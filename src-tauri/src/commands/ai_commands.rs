use crate::ai::cost::ModelPrice;
use crate::ai::import::{self, ImportReport};
use crate::ai::provider::{ConnectionTest, ProviderKind, UsageRecord};
use crate::ai::usage::Analytics;
use crate::ai::{custom, QuickSummary, ProviderView, TestRequestResult};
use crate::database::repository as repo;
use crate::error::{AppError, AppResult};
use crate::security::keychain::{self, KeyKind};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> AppResult<Vec<ProviderView>> {
    state.ai.providers().await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigInput {
    pub id: String,
    pub enabled: bool,
    pub sync_interval_minutes: i64,
    pub base_url: Option<String>,
    pub selected_models: Vec<String>,
}

#[tauri::command]
pub async fn save_provider_config(app: AppHandle, state: State<'_, AppState>, config: ProviderConfigInput) -> AppResult<()> {
    let kind = ProviderKind::parse(&config.id)?;
    if !(5..=24 * 60).contains(&config.sync_interval_minutes) {
        return Err(AppError::Invalid("sync interval must be between 5 minutes and 24 hours".into()));
    }
    let base_url = match (&kind, config.base_url.as_deref().map(str::trim).filter(|s| !s.is_empty())) {
        (ProviderKind::Custom, Some(u)) => Some(custom::validate_base_url(u)?),
        (ProviderKind::Custom, None) if config.enabled => {
            return Err(AppError::Invalid("custom provider needs a base URL".into()))
        }
        _ => None,
    };
    let models: Vec<String> = config
        .selected_models
        .into_iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty() && m.len() < 200)
        .take(100)
        .collect();
    let id = kind.id();
    state
        .db
        .call(move |c| repo::update_provider_config(c, id, config.enabled, config.sync_interval_minutes, base_url.as_deref(), &models))
        .await?;
    let _ = app.emit("ai://providers-changed", id);
    Ok(())
}

#[tauri::command]
pub async fn set_provider_key(app: AppHandle, provider: String, kind: String, secret: String) -> AppResult<()> {
    let p = ProviderKind::parse(&provider)?;
    let k = KeyKind::parse(&kind)?;
    let secret = secret.trim().to_string();
    if secret.len() < 8 || secret.len() > 512 || secret.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::Invalid("that does not look like an API key".into()));
    }
    tokio::task::spawn_blocking(move || keychain::set(p.id(), k, &secret)).await??;
    tracing::info!(provider = p.id(), kind = k.as_str(), "credential saved to Keychain");
    let _ = app.emit("ai://providers-changed", p.id());
    Ok(())
}

#[tauri::command]
pub async fn remove_provider_key(app: AppHandle, provider: String, kind: String) -> AppResult<()> {
    let p = ProviderKind::parse(&provider)?;
    let k = KeyKind::parse(&kind)?;
    tokio::task::spawn_blocking(move || keychain::delete(p.id(), k)).await??;
    let _ = app.emit("ai://providers-changed", p.id());
    Ok(())
}

/// Remove both keys from the Keychain and reset the provider's sync state.
/// Usage history is kept (delete it separately under History & Storage).
#[tauri::command]
pub async fn remove_provider_credentials(app: AppHandle, state: State<'_, AppState>, provider: String) -> AppResult<()> {
    let p = ProviderKind::parse(&provider)?;
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        keychain::delete(p.id(), KeyKind::Api)?;
        keychain::delete(p.id(), KeyKind::Admin)
    })
    .await??;
    let id = p.id();
    state.db.call(move |c| repo::clear_provider_state(c, id)).await?;
    let _ = app.emit("ai://providers-changed", id);
    Ok(())
}

#[tauri::command]
pub async fn test_provider_connection(state: State<'_, AppState>, provider: String) -> AppResult<ConnectionTest> {
    state.ai.test_connection(ProviderKind::parse(&provider)?).await
}

#[tauri::command]
pub async fn sync_provider(state: State<'_, AppState>, provider: String) -> AppResult<String> {
    state.ai.sync_provider(ProviderKind::parse(&provider)?).await
}

#[tauri::command]
pub async fn send_test_request(state: State<'_, AppState>, provider: String, model: String) -> AppResult<TestRequestResult> {
    state.ai.send_test_request(ProviderKind::parse(&provider)?, &model).await
}

#[tauri::command]
pub async fn get_ai_analytics(
    state: State<'_, AppState>,
    from: i64,
    to: i64,
    filter: Option<crate::ai::AnalyticsFilter>,
) -> AppResult<Analytics> {
    if to <= from {
        return Err(AppError::Invalid("invalid date range".into()));
    }
    state.ai.analytics_filtered(from, to, &filter.unwrap_or_default()).await
}

#[tauri::command]
pub fn get_ai_summary(state: State<'_, AppState>) -> QuickSummary {
    state.ai.summary()
}

#[tauri::command]
pub async fn recent_telemetry(state: State<'_, AppState>, limit: Option<i64>) -> AppResult<Vec<UsageRecord>> {
    let limit = limit.unwrap_or(50).clamp(1, 500);
    state.db.call(move |c| repo::recent_telemetry(c, limit)).await
}

#[tauri::command]
pub async fn list_prices(state: State<'_, AppState>) -> AppResult<Vec<ModelPrice>> {
    state.db.call(|c| repo::list_prices(c)).await
}

#[tauri::command]
pub async fn upsert_price(app: AppHandle, state: State<'_, AppState>, price: ModelPrice) -> AppResult<()> {
    ProviderKind::parse(&price.provider)?;
    if price.model.trim().is_empty() {
        return Err(AppError::Invalid("model is required".into()));
    }
    for v in [price.input_price, price.output_price, price.cached_input_price, price.cache_write_price].into_iter().flatten() {
        if !(0.0..=10_000.0).contains(&v) {
            return Err(AppError::Invalid("prices must be between 0 and 10,000 USD per 1M tokens".into()));
        }
    }
    let mut p = price;
    p.model = p.model.trim().to_string();
    state.db.call(move |c| repo::upsert_price(c, &p)).await?;
    state.ai.refresh_summary().await;
    let _ = app.emit("ai://usage-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn delete_price(app: AppHandle, state: State<'_, AppState>, provider: String, model: String) -> AppResult<()> {
    state.db.call(move |c| repo::delete_price(c, &provider, &model)).await?;
    state.ai.refresh_summary().await;
    let _ = app.emit("ai://usage-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn import_usage_file(app: AppHandle, state: State<'_, AppState>, path: String) -> AppResult<ImportReport> {
    let p = std::path::PathBuf::from(&path);
    let format = match p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
        Some("csv") => "csv",
        Some("json") => "json",
        _ => return Err(AppError::Invalid("choose a .csv or .json file".into())),
    };
    let meta = tokio::fs::metadata(&p).await?;
    if meta.len() > 50 * 1024 * 1024 {
        return Err(AppError::Invalid("file is larger than 50 MB".into()));
    }
    let content = tokio::fs::read_to_string(&p).await?;
    let (records, report) = import::parse(&content, format)?;
    state.db.call(move |c| repo::upsert_usage(c, &records)).await?;
    state.ai.refresh_summary().await;
    let _ = app.emit("ai://usage-changed", ());
    Ok(report)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn get_proxy_status(state: State<'_, AppState>) -> AppResult<ProxyStatus> {
    let s = state.settings();
    let running = state.proxy.lock().await.as_ref().map(|h| h.port);
    Ok(ProxyStatus {
        enabled: s.ai.proxy_enabled,
        running: running.is_some(),
        port: running.unwrap_or(s.ai.proxy_port),
        error: state.proxy_error.lock().unwrap().clone(),
    })
}

/// Destructive: frontend must confirm before calling.
#[tauri::command]
pub async fn delete_usage_source(app: AppHandle, state: State<'_, AppState>, source: String) -> AppResult<usize> {
    if !["telemetry", "imported", "provider_reported"].contains(&source.as_str()) {
        return Err(AppError::Invalid("unknown source".into()));
    }
    let n = state.db.call(move |c| repo::delete_usage_by_source(c, &source)).await?;
    state.ai.refresh_summary().await;
    let _ = app.emit("ai://usage-changed", ());
    Ok(n)
}
