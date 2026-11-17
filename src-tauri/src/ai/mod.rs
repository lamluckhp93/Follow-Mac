//! AI usage monitoring: provider adapters, sync scheduler, telemetry, analytics.

pub mod anthropic;
pub mod cost;
pub mod custom;
pub mod deepseek;
pub mod gemini;
pub mod groq;
pub mod import;
pub mod openai;
pub mod openrouter;
pub mod provider;
pub mod proxy;
pub mod usage;

use crate::alerts::AlertEngine;
use crate::database::repository as repo;
use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::security::keychain::{self, KeyKind};
use crate::security::redact;
use crate::settings::Settings;
use crate::util::{now_ms, utc_day_start, DAY_MS};
use crate::Emitter;
use cost::Pricing;
use provider::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use usage::{Analytics, ExtractedUsage, Totals};

pub fn adapter(kind: ProviderKind) -> Arc<dyn ProviderAdapter> {
    match kind {
        ProviderKind::Openai => Arc::new(openai::OpenAi),
        ProviderKind::Anthropic => Arc::new(anthropic::Anthropic),
        ProviderKind::Gemini => Arc::new(gemini::Gemini),
        ProviderKind::Openrouter => Arc::new(openrouter::OpenRouter),
        ProviderKind::Deepseek => Arc::new(deepseek::DeepSeek),
        ProviderKind::Groq => Arc::new(groq::Groq),
        ProviderKind::Custom => Arc::new(custom::Custom),
    }
}

/// Status shown on each provider card.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected,
    NotConfigured,
    Error,
    Unsupported,
    Syncing,
    Disabled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    #[serde(flatten)]
    pub row: repo::ProviderRow,
    pub has_api_key: bool,
    pub has_admin_key: bool,
    pub status: ConnectionStatus,
    pub capabilities: Capabilities,
    pub proxy_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRequestResult {
    pub ok: bool,
    pub status: u16,
    pub model: String,
    pub usage: Option<UsageSummary>,
    pub message: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub input: i64,
    pub output: i64,
    pub cached: i64,
    pub cache_write: i64,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QuickSummary {
    pub today: Totals,
    pub month: Totals,
    pub connected_providers: usize,
    pub enabled_providers: usize,
}

#[derive(Clone)]
pub struct AiService {
    pub db: Db,
    pub http: reqwest::Client,
    pub settings: Arc<RwLock<Settings>>,
    pub alerts: AlertEngine,
    emit: Emitter,
    syncing: Arc<Mutex<HashSet<ProviderKind>>>,
    summary: Arc<Mutex<QuickSummary>>,
}

impl AiService {
    pub fn new(db: Db, settings: Arc<RwLock<Settings>>, alerts: AlertEngine, emit: Emitter) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("http client");
        Self {
            db,
            http,
            settings,
            alerts,
            emit,
            syncing: Default::default(),
            summary: Default::default(),
        }
    }

    pub async fn context(&self, kind: ProviderKind) -> AppResult<ProviderContext> {
        let id = kind.id();
        let row = self.db.call(move |c| repo::get_provider(c, id)).await?;
        // Keychain reads can block briefly; keep them off the async workers.
        let (api_key, admin_key) =
            tokio::task::spawn_blocking(move || (keychain::get(id, KeyKind::Api), keychain::get(id, KeyKind::Admin))).await?;
        Ok(ProviderContext {
            http: self.http.clone(),
            api_key: api_key?,
            admin_key: admin_key?,
            base_url: row.and_then(|r| r.base_url),
        })
    }

    pub async fn providers(&self) -> AppResult<Vec<ProviderView>> {
        let rows = self.db.call(|c| repo::list_providers(c)).await?;
        let keys = tokio::task::spawn_blocking(|| {
            ProviderKind::ALL
                .iter()
                .map(|k| (keychain::exists(k.id(), KeyKind::Api), keychain::exists(k.id(), KeyKind::Admin)))
                .collect::<Vec<_>>()
        })
        .await?;
        let syncing = self.syncing.lock().unwrap().clone();
        let proxy = self.settings.read().map(|s| (s.ai.proxy_enabled, s.ai.proxy_port)).unwrap_or((false, 0));
        let mut out = vec![];
        for row in rows {
            let Ok(kind) = ProviderKind::parse(&row.id) else { continue };
            let idx = ProviderKind::ALL.iter().position(|k| *k == kind).unwrap();
            let (has_api, has_admin) = keys[idx];
            let caps = adapter(kind).capabilities();
            let configured = has_api || has_admin || (kind == ProviderKind::Custom && row.base_url.is_some());
            let status = if syncing.contains(&kind) {
                ConnectionStatus::Syncing
            } else if !configured {
                ConnectionStatus::NotConfigured
            } else if !row.enabled {
                ConnectionStatus::Disabled
            } else {
                match row.last_sync_status.as_deref() {
                    Some("error") => ConnectionStatus::Error,
                    Some("unsupported") => ConnectionStatus::Unsupported,
                    _ => ConnectionStatus::Connected,
                }
            };
            out.push(ProviderView {
                proxy_base_url: proxy.0.then(|| proxy::client_base_url(kind, proxy.1)),
                has_api_key: has_api,
                has_admin_key: has_admin,
                status,
                capabilities: caps,
                row,
            });
        }
        Ok(out)
    }

    pub async fn test_connection(&self, kind: ProviderKind) -> AppResult<ConnectionTest> {
        let ctx = self.context(kind).await?;
        let result = adapter(kind).test_connection(&ctx).await;
        if let Ok(r) = &result {
            let (status, err) = if r.ok { ("ok", None) } else { ("error", Some(r.message.clone())) };
            let id = kind.id();
            self.db.call(move |c| repo::set_sync_status(c, id, status, err.as_deref())).await?;
        }
        result
    }

    /// Sync official usage for one provider. Concurrent syncs of the same
    /// provider are coalesced.
    pub async fn sync_provider(&self, kind: ProviderKind) -> AppResult<String> {
        if !self.syncing.lock().unwrap().insert(kind) {
            return Ok("sync already in progress".into());
        }
        (self.emit)("ai://providers-changed", serde_json::json!({ "provider": kind.id() }));
        let result = self.sync_inner(kind).await;
        self.syncing.lock().unwrap().remove(&kind);
        let id = kind.id();
        let (status, err): (&str, Option<String>) = match &result {
            Ok(_) => ("ok", None),
            Err(AppError::Unsupported(m)) => ("unsupported", Some(m.clone())),
            Err(AppError::NotConfigured(m)) => ("unsupported", Some(m.clone())),
            Err(e) => ("error", Some(e.to_string())),
        };
        let err2 = err.clone();
        self.db.call(move |c| repo::set_sync_status(c, id, status, err2.as_deref())).await?;
        if status == "error" {
            let auth = matches!(result, Err(AppError::Auth(_)));
            self.alerts.provider_error(kind.id(), err.as_deref().unwrap_or(""), auth).await;
        }
        (self.emit)("ai://providers-changed", serde_json::json!({ "provider": id }));
        (self.emit)("ai://usage-changed", serde_json::Value::Null);
        self.refresh_summary().await;
        result
    }

    async fn sync_inner(&self, kind: ProviderKind) -> AppResult<String> {
        let ctx = self.context(kind).await?;
        let id = kind.id();
        let row = self.db.call(move |c| repo::get_provider(c, id)).await?;
        let now = now_ms();
        // First sync: 30 days. Later: re-read the last 3 days (providers
        // finalise recent buckets late) — upserts make this idempotent.
        let from = match row.and_then(|r| r.last_sync_at) {
            Some(last) => utc_day_start(last) - 2 * DAY_MS,
            None => utc_day_start(now) - 30 * DAY_MS,
        };
        let res = adapter(kind).sync_usage(&ctx, from, now).await.map_err(|e| match e {
            AppError::Provider(m) => AppError::Provider(redact(&m, &ctx.secrets())),
            other => other,
        })?;
        let (nu, nc) = (res.usage.len(), res.costs.len());
        let (usage, costs, account) = (res.usage, res.costs, res.account);
        self.db
            .call(move |c| {
                repo::upsert_usage(c, &usage)?;
                repo::upsert_costs(c, &costs)?;
                if let Some(a) = &account {
                    repo::set_account(c, id, a)?;
                }
                Ok(())
            })
            .await?;
        let mut msg = format!("{nu} usage rows, {nc} cost rows");
        if !res.notes.is_empty() {
            msg.push_str(" · ");
            msg.push_str(&res.notes.join("; "));
        }
        tracing::info!(provider = id, "{msg}");
        Ok(msg)
    }

    /// Store one telemetry observation (never stores prompt/response content).
    pub async fn record_telemetry(&self, provider: &str, u: ExtractedUsage, fallback_model: Option<String>) -> AppResult<()> {
        let ts = now_ms();
        let rec = UsageRecord {
            provider: provider.to_string(),
            model: u.model.or(fallback_model).unwrap_or_else(|| "unknown".into()),
            source: "telemetry".into(),
            bucket_start: ts,
            bucket_end: ts,
            input_tokens: u.input,
            output_tokens: u.output,
            cached_tokens: u.cached,
            cache_write_tokens: u.cache_write,
            reasoning_tokens: u.reasoning,
            requests: 1,
            cost_usd: u.cost_usd,
            cost_source: u.cost_usd.map(|_| "response".to_string()),
            external_id: uuid::Uuid::new_v4().to_string(),
        };
        self.db.call(move |c| repo::upsert_usage(c, &[rec]).map(|_| ())).await?;
        (self.emit)("ai://usage-changed", serde_json::Value::Null);
        self.refresh_summary().await;
        Ok(())
    }

    /// Send a tiny request (≈ 20 tokens) and record its usage as telemetry.
    pub async fn send_test_request(&self, kind: ProviderKind, model: &str) -> AppResult<TestRequestResult> {
        if model.trim().is_empty() {
            return Err(AppError::Invalid("choose a model".into()));
        }
        let ctx = self.context(kind).await?;
        let tr = adapter(kind).test_request(&ctx, model.trim())?;
        let t = std::time::Instant::now();
        let resp = tr.request.header(reqwest::header::USER_AGENT, USER_AGENT).send().await?;
        let status = resp.status();
        let ct = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let body = resp.bytes().await?;
        let latency_ms = t.elapsed().as_millis() as u64;
        if !status.is_success() {
            let msg = extract_error_message(&String::from_utf8_lossy(&body)).unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
            return Ok(TestRequestResult {
                ok: false,
                status: status.as_u16(),
                model: tr.model,
                usage: None,
                message: redact(&msg, &ctx.secrets()),
                latency_ms,
            });
        }
        let mut acc = usage::UsageAccumulator::new(kind.id(), &ct);
        acc.feed(&body);
        match acc.finish() {
            Some(u) => {
                let summary = UsageSummary { input: u.input, output: u.output, cached: u.cached, cache_write: u.cache_write, cost_usd: u.cost_usd };
                self.record_telemetry(kind.id(), u, Some(tr.model.clone())).await?;
                Ok(TestRequestResult { ok: true, status: status.as_u16(), model: tr.model, usage: Some(summary), message: "Usage recorded as telemetry".into(), latency_ms })
            }
            None => Ok(TestRequestResult {
                ok: false,
                status: status.as_u16(),
                model: tr.model,
                usage: None,
                message: "Response contained no usage metadata".into(),
                latency_ms,
            }),
        }
    }

    pub async fn analytics(&self, from: i64, to: i64) -> AppResult<Analytics> {
        self.analytics_filtered(from, to, &AnalyticsFilter::default()).await
    }

    /// Analytics restricted to some providers and/or models. Cost records that
    /// are not attributed to a model are excluded when a model filter is set.
    pub async fn analytics_filtered(&self, from: i64, to: i64, filter: &AnalyticsFilter) -> AppResult<Analytics> {
        let forced = self.settings.read().map(|s| s.ai.usage_source.clone()).unwrap_or_else(|_| "auto".into());
        // Provider buckets are whole UTC days while filters use local time, so a
        // bucket belongs to the range when its midpoint does.
        let (qf, qt) = (from - DAY_MS, to);
        let (records, costs, prices) = self
            .db
            .call(move |c| Ok((repo::usage_in_range(c, qf, qt)?, repo::costs_in_range(c, qf, qt)?, repo::list_prices(c)?)))
            .await?;
        let inside = |s: i64, e: i64| {
            let mid = s + (e.max(s) - s) / 2;
            mid >= from && mid < to
        };
        let records: Vec<_> = records
            .into_iter()
            .filter(|r| inside(r.bucket_start, r.bucket_end) && filter.matches(&r.provider, Some(&r.model)))
            .collect();
        let costs: Vec<_> = costs
            .into_iter()
            .filter(|c| inside(c.bucket_start, c.bucket_end) && filter.matches(&c.provider, c.model.as_deref()))
            .collect();
        let forced = (forced != "auto").then_some(forced);
        Ok(usage::aggregate(&records, &costs, &Pricing::new(prices), from, to, forced.as_deref()))
    }

    pub fn summary(&self) -> QuickSummary {
        self.summary.lock().unwrap().clone()
    }

    /// Recompute today/month totals (local time) for the tray, widget and AI alerts.
    pub async fn refresh_summary(&self) {
        use chrono::{Datelike, Local, TimeZone};
        let now = Local::now();
        let day_start = Local
            .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
            .single()
            .map(|d| d.timestamp_millis())
            .unwrap_or_else(|| utc_day_start(now_ms()));
        let month_start = Local
            .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
            .single()
            .map(|d| d.timestamp_millis())
            .unwrap_or(day_start);
        let end = now_ms() + 1;
        let (today, month) = match (self.analytics(day_start, end).await, self.analytics(month_start, end).await) {
            (Ok(t), Ok(m)) => (t.totals, m.totals),
            (Err(e), _) | (_, Err(e)) => {
                tracing::warn!("summary refresh failed: {e}");
                return;
            }
        };
        let providers = self.db.call(|c| repo::list_providers(c)).await.unwrap_or_default();
        let s = QuickSummary {
            today: today.clone(),
            month: month.clone(),
            enabled_providers: providers.iter().filter(|p| p.enabled).count(),
            connected_providers: providers.iter().filter(|p| p.enabled && p.last_sync_status.as_deref() == Some("ok")).count(),
        };
        *self.summary.lock().unwrap() = s;
        self.alerts.evaluate_ai(&today, &month).await;
    }

    /// Background scheduler: syncs enabled providers when their interval elapses.
    pub async fn run_scheduler(self, cancel: CancellationToken) {
        self.refresh_summary().await;
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut summary_tick = 0u32;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tick.tick() => {}
            }
            let providers = match self.db.call(|c| repo::list_providers(c)).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("scheduler: {e}");
                    continue;
                }
            };
            let now = now_ms();
            for p in providers.into_iter().filter(|p| p.enabled) {
                let Ok(kind) = ProviderKind::parse(&p.id) else { continue };
                let caps = adapter(kind).capabilities();
                if !(caps.usage_api || caps.cost_api || caps.account_info) {
                    continue; // nothing to poll; telemetry/import only
                }
                let due = p.last_sync_at.is_none_or(|t| now - t >= p.sync_interval_minutes.max(5) * 60_000);
                if due {
                    let svc = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = svc.sync_provider(kind).await {
                            tracing::warn!(provider = kind.id(), "scheduled sync failed: {e}");
                        }
                    });
                }
            }
            summary_tick += 1;
            if summary_tick % 4 == 0 {
                // Every 2 minutes: day rollover + AI budget alerts.
                self.refresh_summary().await;
            }
        }
        tracing::info!("AI scheduler stopped");
    }
}

/// Optional analytics filter. Empty lists mean "no restriction".
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyticsFilter {
    pub providers: Vec<String>,
    pub models: Vec<String>,
}

impl AnalyticsFilter {
    pub fn matches(&self, provider: &str, model: Option<&str>) -> bool {
        let p_ok = self.providers.is_empty() || self.providers.iter().any(|p| p == provider);
        let m_ok = self.models.is_empty() || model.is_some_and(|m| self.models.iter().any(|x| x == m));
        p_ok && m_ok
    }
}

#[cfg(test)]
mod filter_tests {
    use super::AnalyticsFilter;

    #[test]
    fn empty_filter_matches_everything() {
        let f = AnalyticsFilter::default();
        assert!(f.matches("openai", Some("gpt-x")));
        assert!(f.matches("openai", None));
    }

    #[test]
    fn provider_and_model_filters() {
        let f = AnalyticsFilter { providers: vec!["anthropic".into()], models: vec![] };
        assert!(f.matches("anthropic", None));
        assert!(!f.matches("openai", Some("m")));
        let f = AnalyticsFilter { providers: vec![], models: vec!["m1".into()] };
        assert!(f.matches("openai", Some("m1")));
        assert!(!f.matches("openai", Some("m2")));
        // Costs without a model cannot be attributed to one.
        assert!(!f.matches("openai", None));
    }
}
