use crate::alerts::notification::{Notifier, TauriNotifier};
use crate::database::repository::{self as repo, AlertEvent, AlertRule};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn list_alert_rules(state: State<'_, AppState>) -> AppResult<Vec<AlertRule>> {
    state.db.call(|c| repo::list_alert_rules(c)).await
}

#[tauri::command]
pub async fn update_alert_rule(state: State<'_, AppState>, rule: AlertRule) -> AppResult<()> {
    if !rule.threshold.is_finite() || rule.threshold < 0.0 {
        return Err(AppError::Invalid("threshold must be ≥ 0".into()));
    }
    if matches!(rule.id.as_str(), "cpu_high" | "memory_high" | "disk_full" | "battery_low") && rule.threshold > 100.0 {
        return Err(AppError::Invalid("percentage threshold must be ≤ 100".into()));
    }
    if !(0..=7 * 86_400).contains(&rule.cooldown_s) || !(0..=86_400).contains(&rule.duration_s) {
        return Err(AppError::Invalid("duration/cooldown out of range".into()));
    }
    state.db.call(move |c| repo::update_alert_rule(c, &rule)).await?;
    state.alerts.reload_rules()?;
    Ok(())
}

#[tauri::command]
pub async fn list_alert_events(state: State<'_, AppState>, limit: Option<i64>) -> AppResult<Vec<AlertEvent>> {
    let limit = limit.unwrap_or(100).clamp(1, 1000);
    state.db.call(move |c| repo::list_alert_events(c, limit)).await
}

/// Destructive: frontend confirms first.
#[tauri::command]
pub async fn clear_alert_events(state: State<'_, AppState>) -> AppResult<usize> {
    state.db.call(|c| repo::clear_alert_events(c)).await
}

#[tauri::command]
pub fn send_test_notification(app: AppHandle) -> AppResult<bool> {
    let n = TauriNotifier { app };
    Ok(n.notify("Mac Observer", "Notifications are working."))
}
