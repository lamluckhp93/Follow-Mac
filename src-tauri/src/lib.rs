//! Mac Observer — macOS system & AI usage dashboard.

pub mod ai;
pub mod alerts;
pub mod commands;
pub mod database;
pub mod error;
pub mod security;
pub mod settings;
pub mod state;
pub mod system;
pub mod tray;
pub mod util;
pub mod windows;

use alerts::notification::TauriNotifier;
use alerts::AlertEngine;
use database::Db;
use settings::Settings;
use state::AppState;
use std::sync::{Arc, Mutex, RwLock};
use system::sampler::{Sampler, SharedBuffers};
use system::SystemSnapshot;
use tauri::{Emitter as _, Manager, RunEvent, WindowEvent};
use tokio_util::sync::CancellationToken;

/// Event sink used by services that must not depend on Tauri directly.
pub type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

fn init_logging(dir: &std::path::Path) -> Option<LogGuard> {
    use tracing_subscriber::prelude::*;
    std::fs::create_dir_all(dir).ok()?;
    let file = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("mac-observer")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .ok()?;
    let (nb, guard) = tracing_appender::non_blocking(file);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,hyper=warn,reqwest=warn,tao=warn,wry=warn"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(nb))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .try_init();
    Some(LogGuard(guard))
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = app.path().app_data_dir()?;
    let log_dir = app.path().app_log_dir()?;
    if let Some(g) = init_logging(&log_dir) {
        app.manage(g);
    }
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Mac Observer starting");

    let db = Db::open(&data_dir.join("mac-observer.db"))?;
    let settings = Arc::new(RwLock::new(Settings::load(&db)?));
    let initial = settings.read().unwrap().clone();

    let emit: Emitter = {
        let h = handle.clone();
        Arc::new(move |event: &str, payload: serde_json::Value| {
            let _ = h.emit(event, payload);
        })
    };
    let alerts = AlertEngine::new(db.clone(), Arc::new(TauriNotifier { app: handle.clone() }), emit.clone());
    let ai = ai::AiService::new(db.clone(), settings.clone(), alerts.clone(), emit.clone());

    let cancel = CancellationToken::new();
    let sys = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_cpu(sysinfo::CpuRefreshKind::nothing()),
    )
    .ok();
    let device = system::sampler::device_info(sys.as_ref());
    let (tx, rx) = tokio::sync::watch::channel(Arc::new(SystemSnapshot::default()));
    let buffers = SharedBuffers::default();
    let sampler = Sampler {
        db: db.clone(),
        settings: settings.clone(),
        tx,
        buffers: buffers.clone(),
        cancel: cancel.clone(),
        fanless: device.fanless,
    }
    .spawn();

    app.manage(AppState {
        db: db.clone(),
        settings: settings.clone(),
        buffers,
        snapshot: rx.clone(),
        device,
        ai: ai.clone(),
        alerts: alerts.clone(),
        cancel: cancel.clone(),
        proxy: tokio::sync::Mutex::new(None),
        proxy_error: Mutex::new(None),
        data_dir,
        log_dir,
        sampler_thread: Mutex::new(Some(sampler)),
    });

    // Menu bar
    let tray = tray::build(&handle, initial.widget.enabled, !initial.monitoring.enabled)?;
    app.manage(tray);

    // Snapshot fan-out: UI event, menu bar, system alerts.
    {
        let handle = handle.clone();
        let mut rx = rx;
        let cancel = cancel.clone();
        let alerts = alerts.clone();
        let ai = ai.clone();
        let settings = settings.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    r = rx.changed() => if r.is_err() { break },
                }
                let snap = rx.borrow_and_update().clone();
                let _ = handle.emit("system://snapshot", &*snap);
                if let Some(t) = handle.try_state::<tray::TrayHandles>() {
                    let cfg = settings.read().map(|s| s.menu_bar.clone()).unwrap_or_default();
                    tray::update(&t, &snap, &ai.summary(), &cfg);
                }
                if snap.monitoring {
                    alerts.evaluate_system(&snap).await;
                }
            }
        });
    }

    // AI sync scheduler
    tauri::async_runtime::spawn(ai.clone().run_scheduler(cancel.clone()));

    // Hourly retention
    {
        let db = db.clone();
        let settings = settings.clone();
        let cancel = cancel.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tick.tick() => {}
                }
                let h = settings.read().map(|s| s.history.clone()).unwrap_or_default();
                match db
                    .call(move |c| database::repository::apply_retention(c, h.system_retention_days, h.ai_retention_days, h.alert_retention_days))
                    .await
                {
                    Ok(n) if n > 0 => tracing::info!(rows = n, "retention removed old rows"),
                    Ok(_) => {}
                    Err(e) => tracing::error!("retention failed: {e}"),
                }
            }
        });
    }

    // Telemetry proxy
    if initial.ai.proxy_enabled {
        let h = handle.clone();
        tauri::async_runtime::spawn(async move {
            let state = h.state::<AppState>();
            commands::settings_commands::restart_proxy(&state).await;
        });
    }

    // Widget
    if initial.widget.enabled {
        if let Err(e) = windows::set_widget(&handle, true, &initial.widget.mode) {
            tracing::error!("could not open widget: {e}");
        }
    }

    // Main window (starts hidden; shown unless launched minimized)
    let launched_minimized = std::env::args().any(|a| a == "--minimized");
    if !(initial.general.start_minimized || launched_minimized) {
        windows::show_main(&handle);
    }
    Ok(())
}

fn shutdown(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        if state.cancel.is_cancelled() {
            return;
        }
        tracing::info!("shutting down");
        state.cancel.cancel();
        if let Some(h) = state.sampler_thread.lock().ok().and_then(|mut g| g.take()) {
            // Sampler flushes pending metrics before exiting.
            let _ = h.join();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri_plugin_window_state::StateFlags;
    let app = tauri::Builder::default()
        // Must be registered first: a second launch only focuses the running
        // instance (two samplers would duplicate history rows and alerts).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tracing::info!("second launch detected; focusing the running instance");
            windows::show_main(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .setup(setup)
        .on_window_event(|window, event| {
            if window.label() == windows::MAIN {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    // Keep running in the menu bar.
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system_commands::get_device_info,
            commands::system_commands::get_snapshot,
            commands::system_commands::get_system_history,
            commands::settings_commands::get_settings,
            commands::settings_commands::update_settings,
            commands::settings_commands::get_app_info,
            commands::settings_commands::set_widget_mode,
            commands::settings_commands::show_main_window,
            commands::ai_commands::list_providers,
            commands::ai_commands::save_provider_config,
            commands::ai_commands::set_provider_key,
            commands::ai_commands::remove_provider_key,
            commands::ai_commands::remove_provider_credentials,
            commands::ai_commands::test_provider_connection,
            commands::ai_commands::sync_provider,
            commands::ai_commands::send_test_request,
            commands::ai_commands::get_ai_analytics,
            commands::ai_commands::get_ai_summary,
            commands::ai_commands::recent_telemetry,
            commands::ai_commands::list_prices,
            commands::ai_commands::upsert_price,
            commands::ai_commands::delete_price,
            commands::ai_commands::import_usage_file,
            commands::ai_commands::get_proxy_status,
            commands::ai_commands::delete_usage_source,
            commands::alert_commands::list_alert_rules,
            commands::alert_commands::update_alert_rule,
            commands::alert_commands::list_alert_events,
            commands::alert_commands::clear_alert_events,
            commands::alert_commands::send_test_notification,
            commands::data_commands::get_storage_stats,
            commands::data_commands::run_retention_now,
            commands::data_commands::backup_database,
            commands::data_commands::export_system_csv,
            commands::data_commands::export_ai_csv,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Mac Observer");

    app.run(|app, event| match event {
        RunEvent::ExitRequested { .. } | RunEvent::Exit => shutdown(app),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => windows::show_main(app),
        _ => {}
    });
}
