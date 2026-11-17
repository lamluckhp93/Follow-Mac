//! Any OpenAI-compatible endpoint (LM Studio, Ollama, vLLM, Together, Azure
//! OpenAI-compatible gateways…). The base URL must include the version
//! segment, e.g. `http://localhost:11434/v1`.

use super::provider::*;
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use std::time::Instant;

pub struct Custom;

pub fn validate_base_url(s: &str) -> AppResult<String> {
    let u = url::Url::parse(s.trim()).map_err(|e| AppError::Invalid(format!("invalid base URL: {e}")))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(AppError::Invalid("base URL must be http(s)".into()));
    }
    if u.scheme() == "http" && !matches!(u.host_str(), Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")) {
        return Err(AppError::Invalid("plain http is only allowed for localhost; use https".into()));
    }
    if u.username() != "" || u.password().is_some() {
        return Err(AppError::Invalid("do not put credentials in the URL; use the API key field".into()));
    }
    Ok(s.trim().trim_end_matches('/').to_string())
}

fn base(ctx: &ProviderContext) -> AppResult<String> {
    let b = ctx.base_url.as_deref().ok_or_else(|| AppError::NotConfigured("base URL not set".into()))?;
    validate_base_url(b)
}

#[async_trait]
impl ProviderAdapter for Custom {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Custom
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            usage_api: false,
            cost_api: false,
            account_info: false,
            requires_admin_key_for_usage: false,
            telemetry: true,
            needs_base_url: true,
            notes: "Any OpenAI-compatible server. Usage is captured via telemetry from the standard `usage` field. API key is optional for local servers.",
            admin_key_label: None,
            docs_url: "https://platform.openai.com/docs/api-reference/chat",
        }
    }

    fn upstream_base(&self, ctx: &ProviderContext) -> AppResult<String> {
        base(ctx)
    }

    async fn test_connection(&self, ctx: &ProviderContext) -> AppResult<ConnectionTest> {
        let t = Instant::now();
        let b = base(ctx)?;
        let key = ctx.api_key.clone();
        Ok(
            match send_json(
                || {
                    let r = ctx.http.get(format!("{b}/models"));
                    match &key {
                        Some(k) => r.bearer_auth(k),
                        None => r,
                    }
                },
                &ctx.secrets(),
            )
            .await
            {
                Ok(v) => {
                    let models = models_from_openai_list(&v);
                    ConnectionTest { ok: true, message: format!("Endpoint reachable · {} models", models.len()), models, latency_ms: t.elapsed().as_millis() as u64, usage_api: None }
                }
                Err(e) => ConnectionTest { ok: false, message: e.to_string(), models: vec![], latency_ms: t.elapsed().as_millis() as u64, usage_api: None },
            },
        )
    }

    fn test_request(&self, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest> {
        let b = base(ctx)?;
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "Reply with the single word: pong"}],
        });
        let mut r = ctx.http.post(format!("{b}/chat/completions")).json(&body);
        if let Some(k) = &ctx.api_key {
            r = r.bearer_auth(k);
        }
        Ok(TestRequest { request: r, model: model.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base_url_rules() {
        assert!(validate_base_url("http://localhost:11434/v1").is_ok());
        assert!(validate_base_url("https://api.together.xyz/v1/").unwrap().ends_with("/v1"));
        assert!(validate_base_url("http://example.com/v1").is_err());
        assert!(validate_base_url("https://user:pw@example.com").is_err());
        assert!(validate_base_url("ftp://x").is_err());
    }
}
