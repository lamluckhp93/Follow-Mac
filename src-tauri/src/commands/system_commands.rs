use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::system::history::{self, MetricPoint};
use crate::system::{DeviceInfo, SystemSnapshot};
use crate::util::now_ms;
use tauri::State;

#[tauri::command]
pub fn get_device_info(state: State<'_, AppState>) -> DeviceInfo {
    state.device.clone()
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> SystemSnapshot {
    (**state.snapshot.borrow()).clone()
}

const MAX_POINTS: usize = 360;

/// History for the last `minutes` (1 … 1440). Short ranges come from the
/// in-memory ring buffer at sample resolution; longer ranges from SQLite.
#[tauri::command]
pub async fn get_system_history(state: State<'_, AppState>, minutes: u32) -> AppResult<Vec<MetricPoint>> {
    if !(1..=60 * 24 * 31).contains(&minutes) {
        return Err(AppError::Invalid("minutes must be between 1 and 44640".into()));
    }
    let to = now_ms();
    let from = to - minutes as i64 * 60_000;
    let ring: Vec<MetricPoint> = state.buffers.ring.lock().map(|r| r.iter().cloned().collect()).unwrap_or_default();
    let ring_covers = ring.first().is_some_and(|p| p.ts <= from + 5_000);

    let points: Vec<MetricPoint> = if minutes <= 15 || (minutes <= 60 && ring_covers) {
        ring.into_iter().filter(|p| p.ts >= from).collect()
    } else {
        let mut rows = history::query_range(&state.db, from, to).await?;
        let pending: Vec<MetricPoint> = state.buffers.pending.lock().map(|p| p.clone()).unwrap_or_default();
        let last_db = rows.last().map(|r| r.ts).unwrap_or(i64::MIN);
        rows.extend(pending.into_iter().filter(|p| p.ts > last_db && p.ts >= from));
        // Include the newest live samples that are not yet aggregated.
        let last = rows.last().map(|r| r.ts).unwrap_or(i64::MIN);
        rows.extend(ring.into_iter().filter(|p| p.ts > last));
        rows
    };
    Ok(if points.len() > MAX_POINTS { history::bucketize(&points, from, to, MAX_POINTS) } else { points })
}
