//! OpenAI. Usage & Costs APIs require an **Admin key** (`sk-admin-…`) created at
//! platform.openai.com → Settings → Organization → Admin keys.
//! Docs: https://platform.openai.com/docs/api-reference/usage

use super::provider::*;
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

pub struct OpenAi;

const BASE: &str = "https://api.openai.com";

/// Parse one page of `/v1/organization/usage/completions` (grouped by model).
/// OpenAI's `input_tokens` *includes* cached tokens; we store uncached input
/// separately so totals are never double counted.
pub fn parse_usage_page(v: &Value) -> Vec<UsageRecord> {
    let mut out = Vec::new();
    for bucket in v.get("data").and_then(Value::as_array).into_iter().flatten() {
        let start = bucket.get("start_time").and_then(Value::as_i64).unwrap_or(0) * 1000;
        let end = bucket.get("end_time").and_then(Value::as_i64).unwrap_or(0) * 1000;
        for r in bucket.get("results").and_then(Value::as_array).into_iter().flatten() {
            let n = |k: &str| r.get(k).and_then(Value::as_i64).unwrap_or(0);
            let model = r.get("model").and_then(Value::as_str).unwrap_or("unknown").to_string();
            let cached = n("input_cached_tokens");
            let input_total = n("input_tokens");
            let batch = r.get("batch").and_then(Value::as_bool).map(|b| if b { ":batch" } else { "" }).unwrap_or("");
            out.push(UsageRecord {
                provider: "openai".into(),
                model: model.clone(),
                source: "provider_reported".into(),
                bucket_start: start,
                bucket_end: end,
                input_tokens: (input_total - cached).max(0),
                output_tokens: n("output_tokens"),
                cached_tokens: cached,
                cache_write_tokens: 0,
                reasoning_tokens: 0,
                requests: n("num_model_requests"),
                cost_usd: None,
                cost_source: None,
                external_id: format!("usage:{start}:{model}{batch}"),
            });
        }
    }
    out
}

/// Parse one page of `/v1/organization/costs` grouped by line item.
/// Line items look like `"gpt-4o-2024-08-06, input"`; the model is the part before the comma.
pub fn parse_cost_page(v: &Value) -> Vec<CostRecord> {
    let mut out = Vec::new();
    for bucket in v.get("data").and_then(Value::as_array).into_iter().flatten() {
        let start = bucket.get("start_time").and_then(Value::as_i64).unwrap_or(0) * 1000;
        let end = bucket.get("end_time").and_then(Value::as_i64).unwrap_or(0) * 1000;
        for (i, r) in bucket.get("results").and_then(Value::as_array).into_iter().flatten().enumerate() {
            let amount = r.pointer("/amount/value").and_then(Value::as_f64).unwrap_or(0.0);
            let line = r.get("line_item").and_then(Value::as_str).map(str::to_string);
            let model = line.as_deref().and_then(|l| l.split(',').next()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            let project = r.get("project_id").and_then(Value::as_str).unwrap_or("");
            out.push(CostRecord {
                provider: "openai".into(),
                model,
                source: "provider_reported".into(),
                bucket_start: start,
                bucket_end: end,
                amount_usd: amount,
                external_id: format!("cost:{start}:{}:{project}:{i}", line.clone().unwrap_or_default()),
                line_item: line,
            });
        }
    }
    out
}

async fn paged(
    ctx: &ProviderContext,
    path: &str,
    params: Vec<(&'static str, String)>,
    mut each: impl FnMut(&Value),
) -> AppResult<()> {
    let key = ctx.admin_key()?.to_string();
    let secrets = ctx.secrets();
    let mut page: Option<String> = None;
    for _ in 0..50 {
        let mut q = params.clone();
        if let Some(p) = &page {
            q.push(("page", p.clone()));
        }
        let v = send_json(|| ctx.http.get(format!("{BASE}{path}")).bearer_auth(&key).query(&q), &secrets).await?;
        each(&v);
        match (v.get("has_more").and_then(Value::as_bool), v.get("next_page").and_then(Value::as_str)) {
            (Some(true), Some(n)) => page = Some(n.to_string()),
            _ => return Ok(()),
        }
    }
    Ok(())
}

#[async_trait]
impl ProviderAdapter for OpenAi {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Openai
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            usage_api: true,
            cost_api: true,
            account_info: false,
            requires_admin_key_for_usage: true,
            telemetry: true,
            needs_base_url: false,
            notes: "Official Usage & Costs APIs need an organization Admin key (sk-admin-…). A regular project key is enough for connection tests and telemetry.",
            admin_key_label: Some("Admin key (sk-admin-…)"),
            docs_url: "https://platform.openai.com/docs/api-reference/usage",
        }
    }

    fn upstream_base(&self, _: &ProviderContext) -> AppResult<String> {
        Ok(BASE.into())
    }

    async fn test_connection(&self, ctx: &ProviderContext) -> AppResult<ConnectionTest> {
        let t = Instant::now();
        let mut models = vec![];
        let mut message = String::new();
        let mut ok = true;
        if let Some(key) = ctx.api_key.as_deref() {
            match send_json(|| ctx.http.get(format!("{BASE}/v1/models")).bearer_auth(key), &ctx.secrets()).await {
                Ok(v) => {
                    models = models_from_openai_list(&v);
                    message = format!("API key valid · {} models available", models.len());
                }
                Err(e) => {
                    ok = false;
                    message = e.to_string();
                }
            }
        }
        let usage_api = match ctx.admin_key.as_deref() {
            Some(key) => {
                let start = (chrono::Utc::now().timestamp() - 86_400).to_string();
                Some(
                    match send_json(
                        || {
                            ctx.http
                                .get(format!("{BASE}/v1/organization/usage/completions"))
                                .bearer_auth(key)
                                .query(&[("start_time", start.as_str()), ("limit", "1")])
                        },
                        &ctx.secrets(),
                    )
                    .await
                    {
                        Ok(_) => UsageApiCheck { ok: true, message: "Usage API reachable with admin key".into() },
                        Err(e) => UsageApiCheck { ok: false, message: e.to_string() },
                    },
                )
            }
            None => None,
        };
        if ctx.api_key.is_none() && usage_api.is_none() {
            return Err(AppError::NotConfigured("add an API key or admin key first".into()));
        }
        if ctx.api_key.is_none() {
            ok = usage_api.as_ref().is_some_and(|u| u.ok);
            message = "No API key; admin key checked only".into();
        }
        Ok(ConnectionTest { ok, message, models, latency_ms: t.elapsed().as_millis() as u64, usage_api })
    }

    async fn sync_usage(&self, ctx: &ProviderContext, from_ms: i64, to_ms: i64) -> AppResult<SyncResult> {
        ctx.admin_key().map_err(|_| AppError::NotConfigured("OpenAI usage sync needs an Admin key".into()))?;
        let mut res = SyncResult::default();
        let common = vec![
            ("start_time", (from_ms / 1000).to_string()),
            ("end_time", (to_ms / 1000).to_string()),
            ("bucket_width", "1d".to_string()),
            ("limit", "31".to_string()),
        ];
        let mut q = common.clone();
        q.push(("group_by", "model".into()));
        paged(ctx, "/v1/organization/usage/completions", q, |v| res.usage.extend(parse_usage_page(v))).await?;

        let mut q = common;
        q.push(("group_by", "line_item".into()));
        match paged(ctx, "/v1/organization/costs", q, |v| res.costs.extend(parse_cost_page(v))).await {
            Ok(()) => {}
            Err(e) => res.notes.push(format!("Costs API: {e}")),
        }
        Ok(res)
    }

    fn test_request(&self, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest> {
        openai_chat_test(&format!("{BASE}/v1"), ctx, model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_usage_and_splits_cached() {
        let v = json!({"object":"page","data":[{"object":"bucket","start_time":1730419200,"end_time":1730505600,
            "results":[{"object":"organization.usage.completions.result","input_tokens":1000,"output_tokens":200,
            "input_cached_tokens":300,"num_model_requests":5,"model":"gpt-4o-mini-2024-07-18","batch":false}]}],
            "has_more":false,"next_page":null});
        let r = parse_usage_page(&v);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].input_tokens, 700);
        assert_eq!(r[0].cached_tokens, 300);
        assert_eq!(r[0].requests, 5);
        assert_eq!(r[0].bucket_start, 1730419200000);
    }

    #[test]
    fn parses_costs() {
        let v = json!({"data":[{"start_time":1730419200,"end_time":1730505600,"results":[
            {"object":"organization.costs.result","amount":{"value":0.12,"currency":"usd"},"line_item":"gpt-4o-2024-08-06, input","project_id":"proj_1"}]}]});
        let c = parse_cost_page(&v);
        assert_eq!(c[0].model.as_deref(), Some("gpt-4o-2024-08-06"));
        assert!((c[0].amount_usd - 0.12).abs() < 1e-9);
    }
}
