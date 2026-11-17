//! Provider adapter interface. Adding a provider = implementing this trait in
//! a new file and registering it in `ai::registry()`.

use crate::error::{AppError, AppResult};
use crate::security::redact;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Openai,
    Anthropic,
    Gemini,
    Openrouter,
    Deepseek,
    Groq,
    Custom,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 7] = [
        ProviderKind::Openai,
        ProviderKind::Anthropic,
        ProviderKind::Gemini,
        ProviderKind::Openrouter,
        ProviderKind::Deepseek,
        ProviderKind::Groq,
        ProviderKind::Custom,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            ProviderKind::Openai => "openai",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Gemini => "gemini",
            ProviderKind::Openrouter => "openrouter",
            ProviderKind::Deepseek => "deepseek",
            ProviderKind::Groq => "groq",
            ProviderKind::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> AppResult<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.id() == s)
            .ok_or_else(|| AppError::Invalid(format!("unknown provider '{s}'")))
    }
}

/// What a provider integration can do. Shown in the UI so users know which
/// numbers are possible for each provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub usage_api: bool,
    pub cost_api: bool,
    pub account_info: bool,
    pub requires_admin_key_for_usage: bool,
    pub telemetry: bool,
    pub needs_base_url: bool,
    /// Human-readable explanation of limitations.
    pub notes: &'static str,
    pub admin_key_label: Option<&'static str>,
    pub docs_url: &'static str,
}

/// Credentials and config for one call. Built per call from Keychain + DB;
/// never stored or serialised.
#[derive(Clone)]
pub struct ProviderContext {
    pub http: reqwest::Client,
    pub api_key: Option<String>,
    pub admin_key: Option<String>,
    pub base_url: Option<String>,
}

impl ProviderContext {
    pub fn api_key(&self) -> AppResult<&str> {
        self.api_key.as_deref().ok_or_else(|| AppError::NotConfigured("API key not set".into()))
    }
    pub fn admin_key(&self) -> AppResult<&str> {
        self.admin_key.as_deref().ok_or_else(|| AppError::NotConfigured("admin key not set".into()))
    }
    pub fn secrets(&self) -> Vec<&str> {
        [self.api_key.as_deref(), self.admin_key.as_deref()].into_iter().flatten().collect()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTest {
    pub ok: bool,
    pub message: String,
    pub models: Vec<String>,
    pub latency_ms: u64,
    pub usage_api: Option<UsageApiCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageApiCheck {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub provider: String,
    pub model: String,
    pub source: String,
    pub bucket_start: i64,
    pub bucket_end: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub cache_write_tokens: i64,
    pub reasoning_tokens: i64,
    pub requests: i64,
    pub cost_usd: Option<f64>,
    pub cost_source: Option<String>,
    pub external_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostRecord {
    pub provider: String,
    pub model: Option<String>,
    pub source: String,
    pub bucket_start: i64,
    pub bucket_end: i64,
    pub amount_usd: f64,
    pub line_item: Option<String>,
    pub external_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub balance_usd: Option<f64>,
    pub balance_label: Option<String>,
    pub credits_used_usd: Option<f64>,
    pub credit_limit_usd: Option<f64>,
    pub used_today_usd: Option<f64>,
    pub used_month_usd: Option<f64>,
    pub fetched_at: i64,
}

#[derive(Debug, Default)]
pub struct SyncResult {
    pub usage: Vec<UsageRecord>,
    pub costs: Vec<CostRecord>,
    pub account: Option<AccountInfo>,
    pub notes: Vec<String>,
}

/// A minimal request used by the "Send test request" feature. The response's
/// usage metadata is recorded as telemetry.
pub struct TestRequest {
    pub request: reqwest::RequestBuilder,
    pub model: String,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> Capabilities;

    /// Upstream base used by the local proxy, e.g. `https://api.openai.com`.
    fn upstream_base(&self, ctx: &ProviderContext) -> AppResult<String>;

    /// Validate credentials without spending tokens.
    async fn test_connection(&self, ctx: &ProviderContext) -> AppResult<ConnectionTest>;

    /// Fetch official usage/cost for `[from_ms, to_ms)`. Default: unsupported.
    async fn sync_usage(&self, _ctx: &ProviderContext, _from_ms: i64, _to_ms: i64) -> AppResult<SyncResult> {
        Err(AppError::Unsupported("this provider has no usage API".into()))
    }

    /// Build a tiny completion request (a few tokens) for the given model.
    fn test_request(&self, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest>;
}

// ---------------------------------------------------------------------------
// Shared HTTP helpers
// ---------------------------------------------------------------------------

pub const USER_AGENT: &str = concat!("MacObserver/", env!("CARGO_PKG_VERSION"));

/// Send a request with bounded retries for 429 / 5xx / transport errors.
/// `build` is called for each attempt (RequestBuilder is not reusable).
pub async fn send_json<F>(build: F, secrets: &[&str]) -> AppResult<serde_json::Value>
where
    F: Fn() -> reqwest::RequestBuilder,
{
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let res = build().header(reqwest::header::USER_AGENT, USER_AGENT).send().await;
        match res {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    let text = resp.text().await?;
                    return serde_json::from_str(&text)
                        .map_err(|e| AppError::Provider(format!("invalid JSON from provider: {e}")));
                }
                let retry_after = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());
                let body = resp.text().await.unwrap_or_default();
                let msg = redact(&extract_error_message(&body).unwrap_or_else(|| body.clone()), secrets);
                let msg = format!("HTTP {}: {}", status.as_u16(), msg);
                let retryable = status.as_u16() == 429 || status.is_server_error();
                if retryable && attempt < 3 {
                    let wait = retry_after.unwrap_or(2u64.pow(attempt)).min(30);
                    tracing::warn!(status = status.as_u16(), wait, "provider request failed, retrying");
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    continue;
                }
                return Err(match status.as_u16() {
                    401 | 403 => AppError::Auth(msg),
                    429 => AppError::RateLimited(msg),
                    404 => AppError::Unsupported(msg),
                    _ => AppError::Provider(msg),
                });
            }
            Err(e) => {
                if attempt < 3 && (e.is_timeout() || e.is_connect()) {
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                    continue;
                }
                return Err(e.into());
            }
        }
    }
}

/// Pull a human message out of common provider error envelopes.
pub fn extract_error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    ["/error/message", "/error", "/message", "/detail"]
        .into_iter()
        .find_map(|p| v.pointer(p).and_then(|c| c.as_str()).map(str::to_string))
}

pub fn models_from_openai_list(v: &serde_json::Value) -> Vec<String> {
    let mut m: Vec<String> = v
        .get("data")
        .and_then(|d| d.as_array())
        .map(|a| a.iter().filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(str::to_string)).collect())
        .unwrap_or_default();
    m.sort();
    m
}

pub fn openai_chat_test(base: &str, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest> {
    let key = ctx.api_key()?.to_string();
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 16,
        "messages": [{"role": "user", "content": "Reply with the single word: pong"}],
    });
    Ok(TestRequest {
        request: ctx.http.post(format!("{}/chat/completions", base.trim_end_matches('/'))).bearer_auth(key).json(&body),
        model: model.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_error_envelopes() {
        assert_eq!(extract_error_message(r#"{"error":{"message":"bad"}}"#).as_deref(), Some("bad"));
        assert_eq!(extract_error_message(r#"{"error":"nope"}"#).as_deref(), Some("nope"));
        assert_eq!(extract_error_message("<html>"), None);
    }
    #[test]
    fn kind_roundtrip() {
        for k in ProviderKind::ALL {
            assert_eq!(ProviderKind::parse(k.id()).unwrap(), k);
        }
    }
}
