//! macOS menu bar item: compact live title + a dropdown with live details.

use crate::ai::QuickSummary;
use crate::settings::MenuBarSettings;
use crate::state::AppState;
use crate::system::SystemSnapshot;
use crate::windows;
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

pub struct TrayHandles {
    pub tray: TrayIcon<Wry>,
    pub sys_line: MenuItem<Wry>,
    pub sys_line2: MenuItem<Wry>,
    pub ai_today: MenuItem<Wry>,
    pub ai_month: MenuItem<Wry>,
    pub widget: CheckMenuItem<Wry>,
    pub pause: CheckMenuItem<Wry>,
    pub last_title: Mutex<String>,
}

pub fn fmt_tokens(n: i64) -> String {
    let f = n as f64;
    if f >= 1e9 {
        format!("{:.1}B", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.1}M", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.1}K", f / 1e3)
    } else {
        n.to_string()
    }
}

pub fn fmt_rate(bps: Option<f64>) -> String {
    match bps {
        None => "—".into(),
        Some(b) if b >= 1e6 => format!("{:.1} MB/s", b / 1e6),
        Some(b) if b >= 1e3 => format!("{:.0} KB/s", b / 1e3),
        Some(b) => format!("{:.0} B/s", b),
    }
}

fn pct(v: Option<f32>) -> String {
    v.map(|v| format!("{:.0}%", v)).unwrap_or_else(|| "—".into())
}

pub fn title(s: &SystemSnapshot, ai: &QuickSummary, cfg: &MenuBarSettings) -> String {
    let mut parts = vec![];
    if cfg.show_status && !s.monitoring {
        parts.push("⏸".to_string());
    }
    if cfg.show_cpu {
        parts.push(format!("CPU {}", pct(s.cpu.usage)));
    }
    if cfg.show_ram {
        parts.push(format!("RAM {}", pct(s.memory.used_percent)));
    }
    if cfg.show_tokens {
        parts.push(format!("AI {}", fmt_tokens(ai.today.total)));
    }
    if cfg.show_status && s.monitoring && parts.is_empty() {
        parts.push("●".into());
    }
    parts.join("  ")
}

pub fn build(app: &AppHandle, widget_on: bool, paused: bool) -> tauri::Result<TrayHandles> {
    let sys_line = MenuItem::with_id(app, "sys", "CPU —  ·  Memory —", false, None::<&str>)?;
    let sys_line2 = MenuItem::with_id(app, "sys2", "Network —  ·  Battery —", false, None::<&str>)?;
    let ai_today = MenuItem::with_id(app, "ai_today", "AI today: —", false, None::<&str>)?;
    let ai_month = MenuItem::with_id(app, "ai_month", "AI this month: —", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Dashboard", true, None::<&str>)?;
    let ai = MenuItem::with_id(app, "open_ai", "AI Usage…", true, None::<&str>)?;
    let widget = CheckMenuItem::with_id(app, "widget", "Floating Widget", true, widget_on, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pause Monitoring", true, paused, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit Mac Observer", true, Some("CmdOrCtrl+Q"))?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[&sys_line, &sys_line2, &sep()?, &ai_today, &ai_month, &sep()?, &open, &ai, &widget, &pause, &settings, &sep()?, &quit],
    )?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .title("Mac Observer")
        .tooltip("Mac Observer")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .build(app)?;

    Ok(TrayHandles { tray, sys_line, sys_line2, ai_today, ai_month, widget, pause, last_title: Mutex::new(String::new()) })
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => windows::open_view(app, "dashboard"),
        "open_ai" => windows::open_view(app, "ai"),
        "settings" => windows::open_view(app, "settings"),
        "quit" => app.exit(0),
        "widget" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let mut s = state.settings();
                s.widget.enabled = !windows::widget_visible(&app);
                if let Err(e) = crate::commands::settings_commands::apply_settings(&app, &state, s).await {
                    tracing::error!("toggle widget: {e}");
                }
            });
        }
        "pause" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let mut s = state.settings();
                s.monitoring.enabled = !s.monitoring.enabled;
                if let Err(e) = crate::commands::settings_commands::apply_settings(&app, &state, s).await {
                    tracing::error!("toggle monitoring: {e}");
                }
            });
        }
        _ => {}
    }
}

pub fn update(h: &TrayHandles, s: &SystemSnapshot, ai: &QuickSummary, cfg: &MenuBarSettings) {
    let t = if cfg.enabled { title(s, ai, cfg) } else { String::new() };
    {
        let mut last = h.last_title.lock().unwrap();
        if *last != t {
            let _ = h.tray.set_title(if t.is_empty() { None } else { Some(t.as_str()) });
            *last = t;
        }
    }
    let mem = match (s.memory.used, s.memory.total) {
        (Some(u), Some(t)) => format!("{:.1}/{:.0} GB ({})", u as f64 / 1_073_741_824.0, t as f64 / 1_073_741_824.0, pct(s.memory.used_percent)),
        _ => "—".into(),
    };
    let _ = h.sys_line.set_text(format!(
        "CPU {}  ·  Memory {}{}",
        pct(s.cpu.usage),
        mem,
        s.memory.pressure_label.as_deref().filter(|l| *l != "normal").map(|l| format!("  ·  pressure {l}")).unwrap_or_default()
    ));
    let battery = match (s.battery.percent, s.battery.charging) {
        (Some(p), Some(true)) => format!("{p:.0}% ⚡"),
        (Some(p), _) => format!("{p:.0}%"),
        _ => match s.battery.status {
            crate::system::MetricStatus::NotPresent => "none".into(),
            _ => "—".into(),
        },
    };
    let _ = h.sys_line2.set_text(format!("↓ {}  ↑ {}  ·  Battery {}", fmt_rate(s.network.rx_bps), fmt_rate(s.network.tx_bps), battery));
    let cost = |t: &crate::ai::usage::Totals| {
        let c = t.actual_cost + t.estimated_cost;
        if t.estimated_cost > 0.0 {
            format!("~${c:.2}")
        } else {
            format!("${c:.2}")
        }
    };
    let _ = h.ai_today.set_text(format!("AI today: {} tokens · {}", fmt_tokens(ai.today.total), cost(&ai.today)));
    let _ = h.ai_month.set_text(format!(
        "AI this month: {} tokens · {}  ({} providers)",
        fmt_tokens(ai.month.total),
        cost(&ai.month),
        ai.enabled_providers
    ));
    let _ = h.pause.set_checked(!s.monitoring);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_format() {
        assert_eq!(fmt_tokens(999), "999");
        assert_eq!(fmt_tokens(1_250), "1.2K");
        assert_eq!(fmt_tokens(3_400_000), "3.4M");
    }
    #[test]
    fn title_respects_settings() {
        let mut s = SystemSnapshot::default();
        s.monitoring = true;
        s.cpu.usage = Some(12.4);
        let cfg = MenuBarSettings { show_ram: false, show_tokens: false, ..Default::default() };
        assert_eq!(title(&s, &QuickSummary::default(), &cfg), "CPU 12%");
    }
}
