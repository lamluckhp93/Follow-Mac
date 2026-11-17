//! All SQL lives here. Functions take `&Connection` so they can run inside
//! `Db::call` closures and be unit-tested against an in-memory DB.

use crate::ai::cost::ModelPrice;
use crate::ai::provider::{AccountInfo, CostRecord, UsageRecord};
use crate::error::AppResult;
use crate::util::now_ms;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- providers

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRow {
    pub id: String,
    pub display_name: String,
    pub enabled: bool,
    pub base_url: Option<String>,
    pub sync_interval_minutes: i64,
    pub selected_models: Vec<String>,
    pub last_sync_at: Option<i64>,
    pub last_sync_status: Option<String>,
    pub last_error: Option<String>,
    pub account: Option<AccountInfo>,
}

fn provider_from_row(r: &Row) -> rusqlite::Result<ProviderRow> {
    let models: String = r.get(5)?;
    let account: Option<String> = r.get(9)?;
    Ok(ProviderRow {
        id: r.get(0)?,
        display_name: r.get(1)?,
        enabled: r.get::<_, i64>(2)? != 0,
        base_url: r.get(3)?,
        sync_interval_minutes: r.get(4)?,
        selected_models: serde_json::from_str(&models).unwrap_or_default(),
        last_sync_at: r.get(6)?,
        last_sync_status: r.get(7)?,
        last_error: r.get(8)?,
        account: account.and_then(|a| serde_json::from_str(&a).ok()),
    })
}

const PROVIDER_COLS: &str = "id, display_name, enabled, base_url, sync_interval_minutes, selected_models_json,
    last_sync_at, last_sync_status, last_error, account_json";

pub fn list_providers(c: &Connection) -> AppResult<Vec<ProviderRow>> {
    let mut st = c.prepare(&format!(
        "SELECT {PROVIDER_COLS} FROM ai_providers ORDER BY CASE id
            WHEN 'openai' THEN 1 WHEN 'anthropic' THEN 2 WHEN 'gemini' THEN 3 WHEN 'openrouter' THEN 4
            WHEN 'deepseek' THEN 5 WHEN 'groq' THEN 6 ELSE 7 END"
    ))?;
    let rows = st.query_map([], provider_from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_provider(c: &Connection, id: &str) -> AppResult<Option<ProviderRow>> {
    Ok(c.query_row(&format!("SELECT {PROVIDER_COLS} FROM ai_providers WHERE id = ?1"), [id], provider_from_row)
        .optional()?)
}

pub fn update_provider_config(
    c: &Connection,
    id: &str,
    enabled: bool,
    sync_interval_minutes: i64,
    base_url: Option<&str>,
    selected_models: &[String],
) -> AppResult<()> {
    c.execute(
        "UPDATE ai_providers SET enabled = ?2, sync_interval_minutes = ?3, base_url = ?4,
            selected_models_json = ?5, updated_at = ?6 WHERE id = ?1",
        params![id, enabled as i64, sync_interval_minutes, base_url, serde_json::to_string(selected_models)?, now_ms()],
    )?;
    Ok(())
}

pub fn set_sync_status(c: &Connection, id: &str, status: &str, error: Option<&str>) -> AppResult<()> {
    c.execute(
        "UPDATE ai_providers SET last_sync_at = ?2, last_sync_status = ?3, last_error = ?4 WHERE id = ?1",
        params![id, now_ms(), status, error],
    )?;
    Ok(())
}

/// Record a connection-test outcome without pretending a sync happened.
pub fn set_status(c: &Connection, id: &str, status: &str, error: Option<&str>) -> AppResult<()> {
    c.execute("UPDATE ai_providers SET last_sync_status = ?2, last_error = ?3 WHERE id = ?1", params![id, status, error])?;
    Ok(())
}

/// Enable a provider when its first credential is saved (custom needs a base URL first).
pub fn enable_on_key_saved(c: &Connection, id: &str) -> AppResult<bool> {
    let n = c.execute(
        "UPDATE ai_providers SET enabled = 1 WHERE id = ?1 AND enabled = 0 AND (id <> 'custom' OR base_url IS NOT NULL)",
        params![id],
    )?;
    Ok(n > 0)
}

pub fn set_account(c: &Connection, id: &str, account: &AccountInfo) -> AppResult<()> {
    c.execute("UPDATE ai_providers SET account_json = ?2 WHERE id = ?1", params![id, serde_json::to_string(account)?])?;
    Ok(())
}

pub fn clear_provider_state(c: &Connection, id: &str) -> AppResult<()> {
    c.execute(
        "UPDATE ai_providers SET enabled = 0, last_sync_at = NULL, last_sync_status = NULL, last_error = NULL,
            account_json = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- usage / cost

/// Insert or replace (by provider+source+external_id). Re-syncing the same
/// period therefore updates rows instead of duplicating them.
pub fn upsert_usage(c: &mut Connection, records: &[UsageRecord]) -> AppResult<usize> {
    let tx = c.transaction()?;
    {
        let mut st = tx.prepare_cached(
            "INSERT INTO ai_usage_records (provider, model, source, bucket_start, bucket_end, input_tokens, output_tokens,
                cached_tokens, cache_write_tokens, reasoning_tokens, requests, cost_usd, cost_source, external_id, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
             ON CONFLICT(provider, source, external_id) DO UPDATE SET
                model = excluded.model, bucket_start = excluded.bucket_start, bucket_end = excluded.bucket_end,
                input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens,
                cached_tokens = excluded.cached_tokens, cache_write_tokens = excluded.cache_write_tokens,
                reasoning_tokens = excluded.reasoning_tokens, requests = excluded.requests,
                cost_usd = excluded.cost_usd, cost_source = excluded.cost_source",
        )?;
        let now = now_ms();
        for r in records {
            st.execute(params![
                r.provider,
                r.model,
                r.source,
                r.bucket_start,
                r.bucket_end,
                r.input_tokens,
                r.output_tokens,
                r.cached_tokens,
                r.cache_write_tokens,
                r.reasoning_tokens,
                r.requests,
                r.cost_usd,
                r.cost_source,
                r.external_id,
                now
            ])?;
        }
    }
    tx.commit()?;
    Ok(records.len())
}

pub fn upsert_costs(c: &mut Connection, records: &[CostRecord]) -> AppResult<usize> {
    let tx = c.transaction()?;
    {
        let mut st = tx.prepare_cached(
            "INSERT INTO ai_cost_records (provider, model, source, bucket_start, bucket_end, amount_usd, line_item, external_id, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(provider, source, external_id) DO UPDATE SET
                model = excluded.model, bucket_start = excluded.bucket_start, bucket_end = excluded.bucket_end,
                amount_usd = excluded.amount_usd, line_item = excluded.line_item",
        )?;
        let now = now_ms();
        for r in records {
            st.execute(params![r.provider, r.model, r.source, r.bucket_start, r.bucket_end, r.amount_usd, r.line_item, r.external_id, now])?;
        }
    }
    tx.commit()?;
    Ok(records.len())
}

fn usage_from_row(r: &Row) -> rusqlite::Result<UsageRecord> {
    Ok(UsageRecord {
        provider: r.get(0)?,
        model: r.get(1)?,
        source: r.get(2)?,
        bucket_start: r.get(3)?,
        bucket_end: r.get(4)?,
        input_tokens: r.get(5)?,
        output_tokens: r.get(6)?,
        cached_tokens: r.get(7)?,
        cache_write_tokens: r.get(8)?,
        reasoning_tokens: r.get(9)?,
        requests: r.get(10)?,
        cost_usd: r.get(11)?,
        cost_source: r.get(12)?,
        external_id: r.get(13)?,
    })
}

const USAGE_COLS: &str = "provider, model, source, bucket_start, bucket_end, input_tokens, output_tokens, cached_tokens,
    cache_write_tokens, reasoning_tokens, requests, cost_usd, cost_source, external_id";

pub fn usage_in_range(c: &Connection, from: i64, to: i64) -> AppResult<Vec<UsageRecord>> {
    let mut st = c.prepare_cached(&format!(
        "SELECT {USAGE_COLS} FROM ai_usage_records WHERE bucket_start >= ?1 AND bucket_start < ?2 ORDER BY bucket_start"
    ))?;
    let rows = st.query_map([from, to], usage_from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn recent_telemetry(c: &Connection, limit: i64) -> AppResult<Vec<UsageRecord>> {
    let mut st = c.prepare_cached(&format!(
        "SELECT {USAGE_COLS} FROM ai_usage_records WHERE source = 'telemetry' ORDER BY bucket_start DESC LIMIT ?1"
    ))?;
    let rows = st.query_map([limit], usage_from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn costs_in_range(c: &Connection, from: i64, to: i64) -> AppResult<Vec<CostRecord>> {
    let mut st = c.prepare_cached(
        "SELECT provider, model, source, bucket_start, bucket_end, amount_usd, line_item, external_id
         FROM ai_cost_records WHERE bucket_start >= ?1 AND bucket_start < ?2 ORDER BY bucket_start",
    )?;
    let rows = st
        .query_map([from, to], |r| {
            Ok(CostRecord {
                provider: r.get(0)?,
                model: r.get(1)?,
                source: r.get(2)?,
                bucket_start: r.get(3)?,
                bucket_end: r.get(4)?,
                amount_usd: r.get(5)?,
                line_item: r.get(6)?,
                external_id: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn delete_usage_by_source(c: &Connection, source: &str) -> AppResult<usize> {
    let n = c.execute("DELETE FROM ai_usage_records WHERE source = ?1", [source])?;
    let m = c.execute("DELETE FROM ai_cost_records WHERE source = ?1", [source])?;
    Ok(n + m)
}

// ---------------------------------------------------------------- pricing

pub fn list_prices(c: &Connection) -> AppResult<Vec<ModelPrice>> {
    let mut st = c.prepare_cached(
        "SELECT provider, model, input_price, output_price, cached_input_price, cache_write_price, price_source, updated_at
         FROM ai_models ORDER BY provider, model",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok(ModelPrice {
                provider: r.get(0)?,
                model: r.get(1)?,
                input_price: r.get(2)?,
                output_price: r.get(3)?,
                cached_input_price: r.get(4)?,
                cache_write_price: r.get(5)?,
                price_source: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn upsert_price(c: &Connection, p: &ModelPrice) -> AppResult<()> {
    c.execute(
        "INSERT INTO ai_models (provider, model, input_price, output_price, cached_input_price, cache_write_price, price_source, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,'user',?7)
         ON CONFLICT(provider, model) DO UPDATE SET input_price = excluded.input_price, output_price = excluded.output_price,
            cached_input_price = excluded.cached_input_price, cache_write_price = excluded.cache_write_price,
            price_source = 'user', updated_at = excluded.updated_at",
        params![p.provider, p.model, p.input_price, p.output_price, p.cached_input_price, p.cache_write_price, now_ms()],
    )?;
    Ok(())
}

pub fn delete_price(c: &Connection, provider: &str, model: &str) -> AppResult<()> {
    c.execute("DELETE FROM ai_models WHERE provider = ?1 AND model = ?2", params![provider, model])?;
    Ok(())
}

// ---------------------------------------------------------------- alerts

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertRule {
    pub id: String,
    pub category: String,
    pub enabled: bool,
    pub threshold: f64,
    pub duration_s: i64,
    pub cooldown_s: i64,
}

pub fn list_alert_rules(c: &Connection) -> AppResult<Vec<AlertRule>> {
    let mut st = c.prepare_cached(
        "SELECT id, category, enabled, threshold, duration_s, cooldown_s FROM alert_rules ORDER BY category DESC, id",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok(AlertRule {
                id: r.get(0)?,
                category: r.get(1)?,
                enabled: r.get::<_, i64>(2)? != 0,
                threshold: r.get(3)?,
                duration_s: r.get(4)?,
                cooldown_s: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_alert_rule(c: &Connection, r: &AlertRule) -> AppResult<()> {
    c.execute(
        "UPDATE alert_rules SET enabled = ?2, threshold = ?3, duration_s = ?4, cooldown_s = ?5, updated_at = ?6 WHERE id = ?1",
        params![r.id, r.enabled as i64, r.threshold, r.duration_s.max(0), r.cooldown_s.max(0), now_ms()],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertEvent {
    pub id: i64,
    pub rule_id: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub value: Option<f64>,
    pub ts: i64,
    pub notified: bool,
}

pub fn insert_alert_event(c: &Connection, e: &AlertEvent) -> AppResult<i64> {
    c.execute(
        "INSERT INTO alert_events (rule_id, severity, title, message, value, ts, notified) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![e.rule_id, e.severity, e.title, e.message, e.value, e.ts, e.notified as i64],
    )?;
    Ok(c.last_insert_rowid())
}

pub fn list_alert_events(c: &Connection, limit: i64) -> AppResult<Vec<AlertEvent>> {
    let mut st = c.prepare_cached(
        "SELECT id, rule_id, severity, title, message, value, ts, notified FROM alert_events ORDER BY ts DESC LIMIT ?1",
    )?;
    let rows = st
        .query_map([limit], |r| {
            Ok(AlertEvent {
                id: r.get(0)?,
                rule_id: r.get(1)?,
                severity: r.get(2)?,
                title: r.get(3)?,
                message: r.get(4)?,
                value: r.get(5)?,
                ts: r.get(6)?,
                notified: r.get::<_, i64>(7)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn last_alert_ts(c: &Connection, rule_id: &str) -> AppResult<Option<i64>> {
    Ok(c.query_row("SELECT MAX(ts) FROM alert_events WHERE rule_id = ?1", [rule_id], |r| r.get(0))?)
}

pub fn clear_alert_events(c: &Connection) -> AppResult<usize> {
    Ok(c.execute("DELETE FROM alert_events", [])?)
}

// ---------------------------------------------------------------- storage

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub system_metrics_rows: i64,
    pub usage_rows: i64,
    pub cost_rows: i64,
    pub alert_event_rows: i64,
    pub oldest_metric_ts: Option<i64>,
    pub db_bytes: u64,
    pub schema_version: i64,
}

pub fn storage_stats(c: &Connection) -> AppResult<StorageStats> {
    let count = |t: &str| -> AppResult<i64> { Ok(c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))?) };
    Ok(StorageStats {
        system_metrics_rows: count("system_metrics")?,
        usage_rows: count("ai_usage_records")?,
        cost_rows: count("ai_cost_records")?,
        alert_event_rows: count("alert_events")?,
        oldest_metric_ts: c.query_row("SELECT MIN(ts) FROM system_metrics", [], |r| r.get(0))?,
        db_bytes: 0,
        schema_version: c.query_row("SELECT COALESCE(MAX(version),0) FROM schema_migrations", [], |r| r.get(0))?,
    })
}

/// Delete data older than the configured retention. 0 days = keep forever.
pub fn apply_retention(c: &Connection, system_days: u32, ai_days: u32, alert_days: u32) -> AppResult<usize> {
    let now = now_ms();
    let cutoff = |d: u32| now - d as i64 * 86_400_000;
    let mut n = 0;
    if system_days > 0 {
        n += c.execute("DELETE FROM system_metrics WHERE ts < ?1", [cutoff(system_days)])?;
    }
    if ai_days > 0 {
        n += c.execute("DELETE FROM ai_usage_records WHERE bucket_start < ?1", [cutoff(ai_days)])?;
        n += c.execute("DELETE FROM ai_cost_records WHERE bucket_start < ?1", [cutoff(ai_days)])?;
    }
    if alert_days > 0 {
        n += c.execute("DELETE FROM alert_events WHERE ts < ?1", [cutoff(alert_days)])?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Db;

    fn rec(id: &str, input: i64) -> UsageRecord {
        UsageRecord {
            provider: "openai".into(),
            model: "gpt".into(),
            source: "provider_reported".into(),
            bucket_start: 1000,
            bucket_end: 2000,
            input_tokens: input,
            output_tokens: 1,
            cached_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            requests: 1,
            cost_usd: None,
            cost_source: None,
            external_id: id.into(),
        }
    }

    #[test]
    fn upsert_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        db.call_sync(|c| {
            upsert_usage(c, &[rec("a", 10), rec("b", 5)])?;
            upsert_usage(c, &[rec("a", 20)])?; // re-sync updates in place
            let rows = usage_in_range(c, 0, 10_000)?;
            assert_eq!(rows.len(), 2);
            assert_eq!(rows.iter().find(|r| r.external_id == "a").unwrap().input_tokens, 20);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn providers_seeded_and_updatable() {
        let db = Db::open_in_memory().unwrap();
        db.call_sync(|c| {
            assert_eq!(list_providers(c)?.len(), 7);
            update_provider_config(c, "groq", true, 15, None, &["llama".into()])?;
            let g = get_provider(c, "groq")?.unwrap();
            assert!(g.enabled);
            assert_eq!(g.selected_models, vec!["llama".to_string()]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn retention_deletes_old_rows() {
        let db = Db::open_in_memory().unwrap();
        db.call_sync(|c| {
            c.execute("INSERT INTO system_metrics (ts) VALUES (?1), (?2)", [0, now_ms()])?;
            let n = apply_retention(c, 7, 0, 90)?;
            assert_eq!(n, 1);
            assert_eq!(storage_stats(c)?.system_metrics_rows, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn alert_rules_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        db.call_sync(|c| {
            let mut r = list_alert_rules(c)?.into_iter().find(|r| r.id == "cpu_high").unwrap();
            r.threshold = 75.0;
            update_alert_rule(c, &r)?;
            assert_eq!(list_alert_rules(c)?.into_iter().find(|r| r.id == "cpu_high").unwrap().threshold, 75.0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn key_save_enables_provider_and_test_status_keeps_sync_time() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).unwrap();
        db.call_sync(|c| {
            assert!(enable_on_key_saved(c, "openai")?);
            assert!(!enable_on_key_saved(c, "openai")?, "already enabled");
            // Custom stays disabled until it has a base URL.
            assert!(!enable_on_key_saved(c, "custom")?);
            let p = get_provider(c, "openai")?.unwrap();
            assert!(p.enabled);
            set_status(c, "openai", "error", Some("Usage API: 401"))?;
            let p = get_provider(c, "openai")?.unwrap();
            assert_eq!(p.last_sync_status.as_deref(), Some("error"));
            assert_eq!(p.last_sync_at, None, "a connection test is not a sync");
            Ok(())
        })
        .unwrap();
    }
}
