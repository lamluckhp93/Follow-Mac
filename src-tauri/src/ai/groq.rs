//! Groq (OpenAI-compatible). No usage API; telemetry captures `usage`
//! (and `x_groq.usage` in streams).

use super::provider::*;
use crate::error::AppResult;
use async_trait::async_trait;
use std::time::Instant;

pub struct Groq;

const BASE: &str = "https://api.groq.com/openai";

#[async_trait]
impl ProviderAdapter for Groq {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Groq
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            usage_api: false,
            cost_api: false,
            account_info: false,
            requires_admin_key_for_usage: false,
            telemetry: true,
            needs_base_url: false,
            notes: "Groq has no public usage API. Tokens are captured via telemetry or import.",
            admin_key_label: None,
            docs_url: "https://console.groq.com/docs/api-reference",
        }
    }

    fn upstream_base(&self, _: &ProviderContext) -> AppResult<String> {
        Ok(BASE.into())
    }

    async fn test_connection(&self, ctx: &ProviderContext) -> AppResult<ConnectionTest> {
        let t = Instant::now();
        let key = ctx.api_key()?;
        Ok(match send_json(|| ctx.http.get(format!("{BASE}/v1/models")).bearer_auth(key), &ctx.secrets()).await {
            Ok(v) => {
                let models = models_from_openai_list(&v);
                ConnectionTest { ok: true, message: format!("API key valid · {} models", models.len()), models, latency_ms: t.elapsed().as_millis() as u64, usage_api: None }
            }
            Err(e) => ConnectionTest { ok: false, message: e.to_string(), models: vec![], latency_ms: t.elapsed().as_millis() as u64, usage_api: None },
        })
    }

    fn test_request(&self, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest> {
        openai_chat_test(&format!("{BASE}/v1"), ctx, model)
    }
}
