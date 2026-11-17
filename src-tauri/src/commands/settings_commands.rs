use crate::error::{AppError, AppResult};
use crate::settings::Settings;
use crate::state::AppState;
use crate::windows;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// Persist settings and apply side effects (autostart, proxy, widget, tray).
pub async fn apply_settings(app: &AppHandle, state: &AppState, new: Settings) -> AppResult<Settings> {
    new.validate()?;
    let old = state.settings();
    new.save(&state.db).await?;
    if let Ok(mut w) = state.settings.write() {
        *w = new.clone();
    }

    if old.general.launch_at_login != new.general.launch_at_login {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.autolaunch();
        let r = if new.general.launch_at_login { al.enable() } else { al.disable() };
        if let Err(e) = r {
            tracing::error!("autostart change failed: {e}");
            return Err(AppError::Other(format!("could not change launch-at-login: {e}")));
        }
    }

    if old.ai.proxy_enabled != new.ai.proxy_enabled || old.ai.proxy_port != new.ai.proxy_port {
        restart_proxy(state).await;
    }

    if old.widget != new.widget || windows::widget_visible(app) != new.widget.enabled {
        windows::set_widget(app, new.widget.enabled, &new.widget.mode).map_err(|e| AppError::Other(e.to_string()))?;
    }
    if let Some(h) = app.try_state::<crate::tray::TrayHandles>() {
        let _ = h.widget.set_checked(new.widget.enabled);
    }

    let _ = app.emit("settings://changed", &new);
    Ok(new)
}

use tauri::Manager;

pub async fn restart_proxy(state: &AppState) {
    let mut guard = state.proxy.lock().await;
    if let Some(h) = guard.take() {
        h.stop();
        // Give the listener a moment to release the port.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let s = state.settings();
    *state.proxy_error.lock().unwrap() = None;
    if s.ai.proxy_enabled {
        match crate::ai::proxy::start(state.ai.clone(), s.ai.proxy_port, &state.cancel).await {
            Ok(h) => *guard = Some(h),
            Err(e) => {
                tracing::error!("proxy start failed: {e}");
                *state.proxy_error.lock().unwrap() = Some(e);
            }
        }
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> AppResult<Settings> {
    apply_settings(&app, &state, settings).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub data_dir: String,
    pub log_dir: String,
    pub database_path: String,
    pub schema_version: i64,
    pub tauri_version: String,
    pub autostart_enabled: Option<bool>,
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    use tauri_plugin_autostart::ManagerExt;
    Ok(AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        data_dir: state.data_dir.display().to_string(),
        log_dir: state.log_dir.display().to_string(),
        database_path: state.db.path().display().to_string(),
        schema_version: state.db.schema_version()?,
        tauri_version: tauri::VERSION.to_string(),
        autostart_enabled: app.autolaunch().is_enabled().ok(),
    })
}

#[tauri::command]
pub fn set_widget_mode(app: AppHandle, mode: String) -> AppResult<()> {
    if mode != "compact" && mode != "expanded" {
        return Err(AppError::Invalid("mode must be compact or expanded".into()));
    }
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app2.state::<AppState>();
        let mut s = state.settings();
        s.widget.mode = mode;
        s.widget.enabled = true;
        if let Err(e) = apply_settings(&app2, &state, s).await {
            tracing::error!("widget mode: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle, view: Option<String>) {
    match view {
        Some(v) => windows::open_view(&app, &v),
        None => windows::show_main(&app),
    }
}
