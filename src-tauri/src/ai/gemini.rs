//! Google Gemini API (AI Studio keys). Google does not offer a public usage
//! endpoint for Gemini API keys — usage lives in Google Cloud Monitoring /
//! AI Studio and requires OAuth. We therefore rely on telemetry and import.

use super::provider::*;
use crate::error::AppResult;
use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

pub struct Gemini;

const BASE: &str = "https://generativelanguage.googleapis.com";

#[async_trait]
impl ProviderAdapter for Gemini {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            usage_api: false,
            cost_api: false,
            account_info: false,
            requires_admin_key_for_usage: false,
            telemetry: true,
            needs_base_url: false,
            notes: "Gemini API keys have no public usage endpoint (usage is only visible in AI Studio / Google Cloud console). Tokens are captured from usageMetadata of requests sent through Mac Observer, or imported.",
            admin_key_label: None,
            docs_url: "https://ai.google.dev/gemini-api/docs/tokens",
        }
    }

    fn upstream_base(&self, _: &ProviderContext) -> AppResult<String> {
        Ok(BASE.into())
    }

    async fn test_connection(&self, ctx: &ProviderContext) -> AppResult<ConnectionTest> {
        let t = Instant::now();
        let key = ctx.api_key()?;
        let r = send_json(
            || ctx.http.get(format!("{BASE}/v1beta/models")).header("x-goog-api-key", key).query(&[("pageSize", "1000")]),
            &ctx.secrets(),
        )
        .await;
        Ok(match r {
            Ok(v) => {
                let mut models: Vec<String> = v
                    .get("models")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|m| {
                        m.get("supportedGenerationMethods")
                            .and_then(Value::as_array)
                            .is_some_and(|a| a.iter().any(|x| x.as_str() == Some("generateContent")))
                    })
                    .filter_map(|m| m.get("name").and_then(Value::as_str))
                    .map(|n| n.trim_start_matches("models/").to_string())
                    .collect();
                models.sort();
                ConnectionTest {
                    ok: true,
                    message: format!("API key valid · {} generative models", models.len()),
                    models,
                    latency_ms: t.elapsed().as_millis() as u64,
                    usage_api: None,
                }
            }
            Err(e) => ConnectionTest {
                ok: false,
                message: e.to_string(),
                models: vec![],
                latency_ms: t.elapsed().as_millis() as u64,
                usage_api: None,
            },
        })
    }

    fn test_request(&self, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest> {
        let key = ctx.api_key()?.to_string();
        let model = model.trim_start_matches("models/");
        let body = serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": "Reply with the single word: pong"}]}],
            "generationConfig": {"maxOutputTokens": 32}
        });
        Ok(TestRequest {
            request: ctx
                .http
                .post(format!("{BASE}/v1beta/models/{model}:generateContent"))
                .header("x-goog-api-key", key)
                .json(&body),
            model: model.into(),
        })
    }
}
