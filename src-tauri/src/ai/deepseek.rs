//! DeepSeek. Only an account balance endpoint exists (`GET /user/balance`);
//! token usage comes from telemetry (`usage.prompt_cache_hit_tokens` etc.).

use super::provider::*;
use crate::error::AppResult;
use crate::util::now_ms;
use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

pub struct DeepSeek;

const BASE: &str = "https://api.deepseek.com";

pub fn parse_balance(v: &Value) -> AccountInfo {
    let infos: Vec<&Value> = v.get("balance_infos").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
    let label = infos
        .iter()
        .filter_map(|i| {
            let cur = i.get("currency").and_then(Value::as_str)?;
            let total = i.get("total_balance").and_then(|t| t.as_str().map(str::to_string).or_else(|| t.as_f64().map(|f| f.to_string())))?;
            Some(format!("{total} {cur}"))
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let usd = infos
        .iter()
        .find(|i| i.get("currency").and_then(Value::as_str) == Some("USD"))
        .and_then(|i| i.get("total_balance"))
        .and_then(|t| t.as_str().and_then(|s| s.parse().ok()).or_else(|| t.as_f64()));
    AccountInfo {
        balance_usd: usd,
        balance_label: (!label.is_empty()).then_some(label),
        fetched_at: now_ms(),
        ..Default::default()
    }
}

#[async_trait]
impl ProviderAdapter for DeepSeek {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Deepseek
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            usage_api: false,
            cost_api: false,
            account_info: true,
            requires_admin_key_for_usage: false,
            telemetry: true,
            needs_base_url: false,
            notes: "DeepSeek exposes the account balance only. Tokens are captured via telemetry or import.",
            admin_key_label: None,
            docs_url: "https://api-docs.deepseek.com/api/get-user-balance",
        }
    }

    fn upstream_base(&self, _: &ProviderContext) -> AppResult<String> {
        Ok(BASE.into())
    }

    async fn test_connection(&self, ctx: &ProviderContext) -> AppResult<ConnectionTest> {
        let t = Instant::now();
        let key = ctx.api_key()?;
        Ok(match send_json(|| ctx.http.get(format!("{BASE}/models")).bearer_auth(key), &ctx.secrets()).await {
            Ok(v) => {
                let models = models_from_openai_list(&v);
                ConnectionTest {
                    ok: true,
                    message: format!("API key valid · {} models", models.len()),
                    models,
                    latency_ms: t.elapsed().as_millis() as u64,
                    usage_api: None,
                }
            }
            Err(e) => ConnectionTest { ok: false, message: e.to_string(), models: vec![], latency_ms: t.elapsed().as_millis() as u64, usage_api: None },
        })
    }

    async fn sync_usage(&self, ctx: &ProviderContext, _from: i64, _to: i64) -> AppResult<SyncResult> {
        let key = ctx.api_key()?;
        let v = send_json(|| ctx.http.get(format!("{BASE}/user/balance")).bearer_auth(key), &ctx.secrets()).await?;
        Ok(SyncResult { account: Some(parse_balance(&v)), notes: vec!["Balance only; no usage API".into()], ..Default::default() })
    }

    fn test_request(&self, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest> {
        openai_chat_test(BASE, ctx, model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn balance() {
        let v = serde_json::json!({"is_available":true,"balance_infos":[{"currency":"USD","total_balance":"4.20","granted_balance":"0","topped_up_balance":"4.20"}]});
        let a = parse_balance(&v);
        assert_eq!(a.balance_usd, Some(4.2));
        assert_eq!(a.balance_label.as_deref(), Some("4.20 USD"));
    }
}
