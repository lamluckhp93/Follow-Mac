//! Anthropic Claude. Usage & cost need an **Admin API key** (`sk-ant-admin…`).
//! Primary: Usage and Cost Analytics API (beta `analytics-usage-cost-2026-08-24`).
//! Fallback: legacy `usage_report/messages` + `cost_report` when the analytics
//! endpoints are not available to the organisation.
//! Docs: https://platform.claude.com/docs/en/manage-claude/usage-cost-analytics-api

use super::provider::*;
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

pub struct Anthropic;

const BASE: &str = "https://api.anthropic.com";
const VERSION: &str = "2026-10-21";
const ANALYTICS_BETA: &str = "analytics-usage-cost-2026-08-24";

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

fn parse_ts(v: Option<&Value>) -> i64 {
    v.and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

fn usage_record(model: &str, start: i64, end: i64, u: &Value, prefix: &str) -> UsageRecord {
    let n = |p: &str| u.pointer(p).and_then(Value::as_i64).unwrap_or(0);
    UsageRecord {
        provider: "anthropic".into(),
        model: model.to_string(),
        source: "provider_reported".into(),
        bucket_start: start,
        bucket_end: end,
        input_tokens: n("/uncached_input_tokens"),
        output_tokens: n("/output_tokens"),
        cached_tokens: n("/cache_read_input_tokens"),
        cache_write_tokens: n("/cache_creation/ephemeral_5m_input_tokens") + n("/cache_creation/ephemeral_1h_input_tokens"),
        reasoning_tokens: 0,
        requests: n("/requests"),
        cost_usd: None,
        cost_source: None,
        external_id: format!("{prefix}:{start}:{model}"),
    }
}

/// Parse a page of `GET /v1/organizations/analytics/usage` grouped by model.
pub fn parse_analytics_usage(v: &Value) -> Vec<UsageRecord> {
    v.get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let model = row.pointer("/group/model").and_then(Value::as_str).unwrap_or("unknown");
            let usage = row.get("usage")?;
            Some(usage_record(model, parse_ts(row.get("starting_at")), parse_ts(row.get("ending_at")), usage, "usage"))
        })
        .collect()
}

/// Parse a page of `GET /v1/organizations/analytics/cost` grouped by model.
/// `amount` is a decimal string in **cents**.
pub fn parse_analytics_cost(v: &Value) -> Vec<CostRecord> {
    v.get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let cents: f64 = row.pointer("/cost/amount")?.as_str()?.parse().ok()?;
            let start = parse_ts(row.get("starting_at"));
            let model = row.pointer("/group/model").and_then(Value::as_str).map(str::to_string);
            let cost_type = row.pointer("/group/cost_type").and_then(Value::as_str).unwrap_or("");
            Some(CostRecord {
                provider: "anthropic".into(),
                external_id: format!("cost:{start}:{}:{cost_type}", model.clone().unwrap_or_default()),
                model,
                source: "provider_reported".into(),
                bucket_start: start,
                bucket_end: parse_ts(row.get("ending_at")),
                amount_usd: cents / 100.0,
                line_item: (!cost_type.is_empty()).then(|| cost_type.to_string()),
            })
        })
        .collect()
}

/// Legacy `usage_report/messages`: `data[].results[]`.
pub fn parse_legacy_usage(v: &Value) -> Vec<UsageRecord> {
    let mut out = vec![];
    for b in v.get("data").and_then(Value::as_array).into_iter().flatten() {
        let (s, e) = (parse_ts(b.get("starting_at")), parse_ts(b.get("ending_at")));
        for r in b.get("results").and_then(Value::as_array).into_iter().flatten() {
            let model = r.get("model").and_then(Value::as_str).unwrap_or("unknown");
            out.push(usage_record(model, s, e, r, "legacy-usage"));
        }
    }
    out
}

/// Legacy `cost_report`: `data[].results[]` with `amount` in cents.
pub fn parse_legacy_cost(v: &Value) -> Vec<CostRecord> {
    let mut out = vec![];
    for b in v.get("data").and_then(Value::as_array).into_iter().flatten() {
        let (s, e) = (parse_ts(b.get("starting_at")), parse_ts(b.get("ending_at")));
        for (i, r) in b.get("results").and_then(Value::as_array).into_iter().flatten().enumerate() {
            let Some(cents) = r.get("amount").and_then(|a| a.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| a.as_f64())) else {
                continue;
            };
            let model = r.get("model").and_then(Value::as_str).map(str::to_string);
            let desc = r.get("description").and_then(Value::as_str).map(str::to_string);
            out.push(CostRecord {
                provider: "anthropic".into(),
                external_id: format!("legacy-cost:{s}:{}:{i}", desc.clone().unwrap_or_default()),
                model,
                source: "provider_reported".into(),
                bucket_start: s,
                bucket_end: e,
                amount_usd: cents / 100.0,
                line_item: desc,
            });
        }
    }
    out
}

fn admin_get(ctx: &ProviderContext, key: &str, path: &str, q: &[(&str, String)], beta: bool) -> reqwest::RequestBuilder {
    let mut r = ctx
        .http
        .get(format!("{BASE}{path}"))
        .header("x-api-key", key)
        .header("anthropic-version", VERSION)
        .query(q);
    if beta {
        r = r.header("anthropic-beta", ANALYTICS_BETA);
    }
    r
}

async fn paged(
    ctx: &ProviderContext,
    path: &str,
    params: Vec<(&str, String)>,
    beta: bool,
    mut each: impl FnMut(&Value),
) -> AppResult<()> {
    let key = ctx.admin_key()?.to_string();
    let secrets = ctx.secrets();
    let mut page: Option<String> = None;
    for _ in 0..100 {
        let mut q = params.clone();
        if let Some(p) = &page {
            q.push(("page", p.clone()));
        }
        let v = send_json(|| admin_get(ctx, &key, path, &q, beta), &secrets).await?;
        each(&v);
        match (v.get("has_more").and_then(Value::as_bool), v.get("next_page").and_then(Value::as_str)) {
            (Some(true), Some(n)) => page = Some(n.to_string()),
            _ => return Ok(()),
        }
    }
    Ok(())
}

#[async_trait]
impl ProviderAdapter for Anthropic {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            usage_api: true,
            cost_api: true,
            account_info: false,
            requires_admin_key_for_usage: true,
            telemetry: true,
            needs_base_url: false,
            notes: "Official usage & cost come from the Usage and Cost Analytics API and need an Admin API key (sk-ant-admin…). Data typically lags a few hours. Not available to individual accounts without an organization.",
            admin_key_label: Some("Admin API key (sk-ant-admin…)"),
            docs_url: "https://platform.claude.com/docs/en/manage-claude/usage-cost-analytics-api",
        }
    }

    fn upstream_base(&self, _: &ProviderContext) -> AppResult<String> {
        Ok(BASE.into())
    }

    async fn test_connection(&self, ctx: &ProviderContext) -> AppResult<ConnectionTest> {
        let t = Instant::now();
        let (mut ok, mut message, mut models) = (true, String::new(), vec![]);
        if let Some(key) = ctx.api_key.as_deref() {
            match send_json(
                || {
                    ctx.http
                        .get(format!("{BASE}/v1/models"))
                        .header("x-api-key", key)
                        .header("anthropic-version", VERSION)
                        .query(&[("limit", "100")])
                },
                &ctx.secrets(),
            )
            .await
            {
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
                let now = crate::util::now_ms();
                let q = vec![("starting_at", iso(now - 3 * 86_400_000)), ("ending_at", iso(now)), ("limit", "1".into())];
                Some(
                    match send_json(|| admin_get(ctx, key, "/v1/organizations/analytics/usage", &q, true), &ctx.secrets()).await {
                        Ok(_) => UsageApiCheck { ok: true, message: "Usage & Cost Analytics API reachable".into() },
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
        ctx.admin_key().map_err(|_| AppError::NotConfigured("Anthropic usage sync needs an Admin API key".into()))?;
        let mut res = SyncResult::default();
        let base = vec![("starting_at", iso(from_ms)), ("ending_at", iso(to_ms))];

        let mut q = base.clone();
        q.extend([
            ("time_bucket_width", "1d".to_string()),
            ("group_by[]", "model".to_string()),
            ("order_by", "time".to_string()),
            ("limit", "1000".to_string()),
        ]);
        let analytics = paged(ctx, "/v1/organizations/analytics/usage", q.clone(), true, |v| {
            res.usage.extend(parse_analytics_usage(v))
        })
        .await;

        match analytics {
            Ok(()) => {
                if let Err(e) =
                    paged(ctx, "/v1/organizations/analytics/cost", q, true, |v| res.costs.extend(parse_analytics_cost(v))).await
                {
                    res.notes.push(format!("Cost analytics: {e}"));
                }
            }
            Err(AppError::Auth(m)) => return Err(AppError::Auth(m)),
            Err(e) => {
                // Fall back to the legacy reports.
                res.notes.push(format!("Analytics API unavailable ({e}); used legacy usage report"));
                res.usage.clear();
                let mut lq = base.clone();
                lq.extend([("bucket_width", "1d".into()), ("group_by[]", "model".into()), ("limit", "31".into())]);
                paged(ctx, "/v1/organizations/usage_report/messages", lq, false, |v| {
                    res.usage.extend(parse_legacy_usage(v))
                })
                .await?;
                let mut cq = base;
                cq.extend([("group_by[]", "description".into()), ("limit", "31".into())]);
                if let Err(e) = paged(ctx, "/v1/organizations/cost_report", cq, false, |v| res.costs.extend(parse_legacy_cost(v))).await {
                    res.notes.push(format!("Legacy cost report: {e}"));
                }
            }
        }
        Ok(res)
    }

    fn test_request(&self, ctx: &ProviderContext, model: &str) -> AppResult<TestRequest> {
        let key = ctx.api_key()?.to_string();
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "Reply with the single word: pong"}],
        });
        Ok(TestRequest {
            request: ctx
                .http
                .post(format!("{BASE}/v1/messages"))
                .header("x-api-key", key)
                .header("anthropic-version", VERSION)
                .json(&body),
            model: model.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_analytics_usage_example() {
        // Shape copied from the official docs example response.
        let v = json!({"data":[{"starting_at":"2026-09-03T00:00:00Z","ending_at":"2026-09-04T00:00:00Z",
            "group":{"model":"claude-sonnet-5"},
            "usage":{"uncached_input_tokens":1284500,"cache_creation":{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":215300},
            "cache_read_input_tokens":3200000,"output_tokens":891000,"total_tokens":5590800,"requests":4210}}],
            "has_more":true,"next_page":"page_x"});
        let r = parse_analytics_usage(&v);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].model, "claude-sonnet-5");
        assert_eq!(r[0].input_tokens, 1284500);
        assert_eq!(r[0].cache_write_tokens, 215300);
        assert_eq!(r[0].cached_tokens, 3200000);
        assert_eq!(r[0].output_tokens, 891000);
        assert_eq!(r[0].requests, 4210);
        // input + cache write + cache read + output == total_tokens
        assert_eq!(r[0].input_tokens + r[0].cache_write_tokens + r[0].cached_tokens + r[0].output_tokens, 5590800);
    }

    #[test]
    fn parses_cost_in_cents() {
        let v = json!({"data":[{"starting_at":"2026-09-01T00:00:00Z","ending_at":"2026-09-02T00:00:00Z",
            "group":{"model":"claude-opus-5"},"cost":{"currency":"USD","amount":"41280.000000","list_amount":"51600.000000"}}]});
        let c = parse_analytics_cost(&v);
        assert!((c[0].amount_usd - 412.80).abs() < 1e-9);
    }

    #[test]
    fn parses_legacy() {
        let v = json!({"data":[{"starting_at":"2025-01-01T00:00:00Z","ending_at":"2025-01-02T00:00:00Z",
            "results":[{"uncached_input_tokens":10,"cache_read_input_tokens":5,"output_tokens":7,"model":"claude-haiku-4-5",
            "cache_creation":{"ephemeral_5m_input_tokens":1,"ephemeral_1h_input_tokens":2}}]}]});
        let r = parse_legacy_usage(&v);
        assert_eq!(r[0].cache_write_tokens, 3);
    }
}
