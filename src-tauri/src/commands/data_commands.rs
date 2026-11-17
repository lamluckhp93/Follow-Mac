use crate::database::repository::{self as repo, StorageStats};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::system::history;
use crate::util::now_ms;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn get_storage_stats(state: State<'_, AppState>) -> AppResult<StorageStats> {
    let mut s = state.db.call(|c| repo::storage_stats(c)).await?;
    let p = state.db.path().to_path_buf();
    let mut bytes = 0;
    for suffix in ["", "-wal", "-shm"] {
        if let Ok(m) = tokio::fs::metadata(format!("{}{}", p.display(), suffix)).await {
            bytes += m.len();
        }
    }
    s.db_bytes = bytes;
    Ok(s)
}

#[tauri::command]
pub async fn run_retention_now(state: State<'_, AppState>) -> AppResult<usize> {
    let h = state.settings().history;
    state
        .db
        .call(move |c| repo::apply_retention(c, h.system_retention_days, h.ai_retention_days, h.alert_retention_days))
        .await
}

fn target(path: &str, ext: &str) -> AppResult<PathBuf> {
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return Err(AppError::Invalid("path must be absolute".into()));
    }
    if p.extension().and_then(|e| e.to_str()) != Some(ext) {
        return Err(AppError::Invalid(format!("file must end with .{ext}")));
    }
    Ok(p)
}

/// Consistent online backup using SQLite `VACUUM INTO`.
#[tauri::command]
pub async fn backup_database(state: State<'_, AppState>, path: String) -> AppResult<String> {
    let p = target(&path, "db")?;
    if p.exists() {
        // The save dialog already asked the user to confirm overwriting.
        tokio::fs::remove_file(&p).await?;
    }
    let ps = p.display().to_string();
    state
        .db
        .call(move |c| {
            c.execute("VACUUM INTO ?1", [ps])?;
            Ok(())
        })
        .await?;
    Ok(p.display().to_string())
}

#[tauri::command]
pub async fn export_system_csv(state: State<'_, AppState>, path: String, days: u32) -> AppResult<usize> {
    let p = target(&path, "csv")?;
    let to = now_ms();
    let from = to - days.clamp(1, 3650) as i64 * 86_400_000;
    let rows = history::query_range(&state.db, from, to).await?;
    let n = rows.len();
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let mut w = csv::Writer::from_path(&p).map_err(|e| AppError::Io(e.to_string()))?;
        w.write_record([
            "timestamp_utc", "cpu_usage_pct", "cpu_user_pct", "cpu_system_pct", "mem_used_bytes", "mem_total_bytes",
            "mem_pressure_level", "swap_used_bytes", "gpu_usage_pct", "disk_read_bps", "disk_write_bps", "net_rx_bps",
            "net_tx_bps", "battery_pct", "battery_charging", "cpu_temp_c",
        ])
        .map_err(|e| AppError::Io(e.to_string()))?;
        let f = |v: Option<f64>| v.map(|x| format!("{x:.3}")).unwrap_or_default();
        for r in rows {
            let ts = chrono::DateTime::from_timestamp_millis(r.ts).map(|d| d.to_rfc3339()).unwrap_or_default();
            w.write_record([
                ts,
                f(r.cpu_usage),
                f(r.cpu_user),
                f(r.cpu_system),
                f(r.mem_used),
                f(r.mem_total),
                f(r.mem_pressure),
                f(r.swap_used),
                f(r.gpu_usage),
                f(r.disk_read_bps),
                f(r.disk_write_bps),
                f(r.net_rx_bps),
                f(r.net_tx_bps),
                f(r.battery_percent),
                f(r.battery_charging),
                f(r.cpu_temp),
            ])
            .map_err(|e| AppError::Io(e.to_string()))?;
        }
        w.flush()?;
        Ok(())
    })
    .await??;
    Ok(n)
}

/// Export all AI usage records in the same format the importer accepts.
#[tauri::command]
pub async fn export_ai_csv(state: State<'_, AppState>, path: String) -> AppResult<usize> {
    let p = target(&path, "csv")?;
    let rows = state.db.call(|c| repo::usage_in_range(c, i64::MIN, i64::MAX)).await?;
    let n = rows.len();
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let mut w = csv::Writer::from_path(&p).map_err(|e| AppError::Io(e.to_string()))?;
        w.write_record([
            "date", "provider", "model", "source", "input_tokens", "output_tokens", "cached_tokens", "cache_write_tokens",
            "reasoning_tokens", "requests", "cost_usd", "cost_source",
        ])
        .map_err(|e| AppError::Io(e.to_string()))?;
        for r in rows {
            let ts = chrono::DateTime::from_timestamp_millis(r.bucket_start).map(|d| d.to_rfc3339()).unwrap_or_default();
            w.write_record([
                ts,
                r.provider,
                r.model,
                r.source,
                r.input_tokens.to_string(),
                r.output_tokens.to_string(),
                r.cached_tokens.to_string(),
                r.cache_write_tokens.to_string(),
                r.reasoning_tokens.to_string(),
                r.requests.to_string(),
                r.cost_usd.map(|c| c.to_string()).unwrap_or_default(),
                r.cost_source.unwrap_or_default(),
            ])
            .map_err(|e| AppError::Io(e.to_string()))?;
        }
        w.flush()?;
        Ok(())
    })
    .await??;
    Ok(n)
}
