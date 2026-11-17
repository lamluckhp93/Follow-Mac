//! OpenRouter. `GET /api/v1/key` reports credit usage for the key (total,
//! daily, weekly, monthly, in USD). Token counts come from telemetry, where
//! OpenRouter responses also include the exact request cost (`usage.cost`).

use super::provider::*;
use crate::error::AppResult;
use crate::util::{now_ms, utc_day_label, utc_day_start, DAY_MS};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

pub struct OpenRouter;

const BASE: &str = "https://openrouter.ai/api";

pub fn parse_key_info(v: &Value) -> AccountInfo {
    let d = v.get("data").unwrap_or(v);
    let f = |k: &str| d.get(k).and_then(Value::as_f64);
    AccountInfo {
        balance_usd: f("limit_remaining"),
        balance_label: d.get("label").and_then(Value::as_str).map(|s| format!("Key: {s}")),
        credits_used_usd: f("usage"),
        credit_limit_usd: f("limit"),
        used_today_usd: f("usage_daily"),
        used_month_usd: f("usage_monthly"),
        fetched_at: now_ms(),
    }
}

#[async_trait]
impl ProviderAdapter for OpenRouter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Openrouter
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            usage_api: false,
            cost_api: true,
            account_info: true,
            requires_admin_key_for_usage: false,
            telemetry: true,
            needs_base_url: false,
            notes: "The key endpoint reports credits spent (today / month / total). Per-request tokens and exact cost are captured via telemetry.",
            admin_key_label: None,
            docs_url: "https://openrouter.ai/docs/api-reference/limits",
        }
    }

    fn upstream_base(&self, _: &ProviderContext) -> AppResult<String> {
        Ok(BASE.into())
    }

    async fn test_connection(&self, ctx: &ProviderContext) -> AppResult<ConnectionTest> {
        let t = Instant::now();
        let key = ctx.api_key()?;
        Ok(match send_json(|| ctx.http.get(format!("{BASE}/v1/key")).bearer_auth(key), &ctx.secrets()).await {
            Ok(v) => {
                let info = parse_key_info(&v);
                let models = send_json(|| ctx.http.get(format!("{BASE}/v1/models")), &[])
                    .await
                    .map(|m| models_from_openai_list(&m))
                    .unwrap_or_default();
                ConnectionTest {
                    ok: true,
                    message: format!(
                        "Key valid · used ${:.4} total{}",
                        info.credits_used_usd.unwrap_or(0.0),
                        info.credit_limit_usd.map(|l| format!(" of ${l:.2} limit")).unwrap_or_default()
                    ),
                    models,
                    latency_ms: t.elapsed().as_millis() as u64,
                    usage_api: Some(UsageApiCheck { ok: true, message: "Credit usage endpoint reachable".into() }),
                }
            }
            Err(e) => ConnectionTest { ok: false, message: e.to_string(), models: vec![], latency_ms: t.elapsed().as_millis() as u64, usage_api: None },
        })
    }

    async fn sync_usage(&self, ctx: &ProviderContext, _from: i64, _to: i64) -> AppResult<SyncResult> {
        let key = ctx.api_key()?;
        let v = send_json(|| ctx.http.get(format!("{BASE}/v1/key")).bearer_auth(key), &ctx.secrets()).await?;
        let info = parse_key_info(&v);
        let mut res = SyncResult::default();
        if let Some(today) = info.used_today_usd {
            let start = utc_day_start(now_ms());
            res.costs.push(CostRecord {
                provider: "openrouter".into(),
                model: None,
                source: "provider_reported".into(),
                bucket_start: start,
                bucket_end: start + DAY_MS,
                amount_usd: today,
                line_item: Some("credits used today (UTC)".into()),
                external_id: format!("daily:{}", utc_day_label(start)),
            });
        } else {
            res.notes.push("Key endpoint did not include daily usage".into());
        }
        res.account = Some(info);
        Ok(res)
    }

    fn test_request(&self, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest> {
        openai_chat_test(&format!("{BASE}/v1"), ctx, model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_info() {
        let v = serde_json::json!({"data":{"label":"sk-or-v1-abc...","usage":1.5,"limit":10.0,"limit_remaining":8.5,"usage_daily":0.25,"usage_monthly":1.2}});
        let i = parse_key_info(&v);
        assert_eq!(i.used_today_usd, Some(0.25));
        assert_eq!(i.balance_usd, Some(8.5));
    }
}
