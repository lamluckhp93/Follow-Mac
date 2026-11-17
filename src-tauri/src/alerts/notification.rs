/// Abstraction over native notifications so the engine is testable.
pub trait Notifier: Send + Sync {
    /// Returns true if the notification was handed to the OS.
    fn notify(&self, title: &str, body: &str) -> bool;
}

/// macOS Notification Center via tauri-plugin-notification.
pub struct TauriNotifier {
    pub app: tauri::AppHandle,
}

impl Notifier for TauriNotifier {
    fn notify(&self, title: &str, body: &str) -> bool {
        use tauri_plugin_notification::NotificationExt;
        match self.app.notification().builder().title(title).body(body).show() {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("notification failed: {e}");
                false
            }
        }
    }
}
