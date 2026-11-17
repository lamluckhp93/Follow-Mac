//! Window management: main dashboard and the floating widget.

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const MAIN: &str = "main";
pub const WIDGET: &str = "widget";

pub fn widget_size(mode: &str) -> (f64, f64) {
    if mode == "expanded" {
        (360.0, 460.0)
    } else {
        (248.0, 168.0)
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Show the main window and ask the frontend to navigate to `view`.
pub fn open_view(app: &AppHandle, view: &str) {
    show_main(app);
    let _ = app.emit_to(MAIN, "app://navigate", view);
}

pub fn widget_visible(app: &AppHandle) -> bool {
    app.get_webview_window(WIDGET).and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

pub fn set_widget(app: &AppHandle, enabled: bool, mode: &str) -> tauri::Result<()> {
    match (enabled, app.get_webview_window(WIDGET)) {
        (true, Some(w)) => {
            let (wd, ht) = widget_size(mode);
            let size = w.inner_size()?;
            let scale = w.scale_factor()?;
            // Only resize when switching modes (keep user resizes otherwise).
            let cur = size.to_logical::<f64>(scale);
            let other = widget_size(if mode == "expanded" { "compact" } else { "expanded" });
            if (cur.width - other.0).abs() < 1.0 && (cur.height - other.1).abs() < 1.0 {
                w.set_size(tauri::LogicalSize::new(wd, ht))?;
            }
            w.show()?;
        }
        (true, None) => {
            let (wd, ht) = widget_size(mode);
            WebviewWindowBuilder::new(app, WIDGET, WebviewUrl::App("index.html#/widget".into()))
                .title("Mac Observer Widget")
                .inner_size(wd, ht)
                .min_inner_size(220.0, 150.0)
                .max_inner_size(640.0, 900.0)
                .decorations(false)
                .transparent(true)
                .shadow(true)
                .always_on_top(true)
                .visible_on_all_workspaces(true)
                .skip_taskbar(true)
                .resizable(true)
                .focused(false)
                .build()?;
        }
        (false, Some(w)) => {
            w.close()?;
        }
        (false, None) => {}
    }
    Ok(())
}
