//! User settings persisted as one JSON document in `application_settings`.
//! Unknown/missing fields fall back to defaults, so older documents keep working.

use crate::database::Db;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub default_view: String, // "dashboard" | "ai" | "history" | "alerts"
    pub language: String,     // "en" | "vi"
}
impl Default for GeneralSettings {
    fn default() -> Self {
        Self { launch_at_login: false, start_minimized: false, default_view: "dashboard".into(), language: "en".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppearanceSettings {
    pub theme: String,  // "dark" (only dark is designed; "system" reserved)
    pub accent: String, // "cyan" | "blue" | "purple"
    pub compact: bool,
    pub animations: bool,
}
impl Default for AppearanceSettings {
    fn default() -> Self {
        Self { theme: "dark".into(), accent: "cyan".into(), compact: false, animations: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MonitoringSettings {
    pub enabled: bool,
    pub cpu_interval_ms: u64,
    pub memory_interval_ms: u64,
    pub network_interval_ms: u64,
    pub disk_interval_ms: u64,
    pub battery_interval_ms: u64,
    pub sensors_interval_ms: u64,
    pub process_interval_ms: u64,
    pub persist_interval_s: u64,
}
impl Default for MonitoringSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            cpu_interval_ms: 2000,
            memory_interval_ms: 2000,
            network_interval_ms: 2000,
            disk_interval_ms: 4000,
            battery_interval_ms: 30_000,
            sensors_interval_ms: 5000,
            process_interval_ms: 5000,
            persist_interval_s: 10,
        }
    }
}
impl MonitoringSettings {
    /// Clamp to sane bounds so a bad value cannot spin the CPU.
    pub fn sanitized(&self) -> Self {
        let c = |v: u64, lo: u64, hi: u64| v.clamp(lo, hi);
        Self {
            enabled: self.enabled,
            cpu_interval_ms: c(self.cpu_interval_ms, 500, 60_000),
            memory_interval_ms: c(self.memory_interval_ms, 500, 60_000),
            network_interval_ms: c(self.network_interval_ms, 500, 60_000),
            disk_interval_ms: c(self.disk_interval_ms, 1000, 120_000),
            battery_interval_ms: c(self.battery_interval_ms, 5000, 600_000),
            sensors_interval_ms: c(self.sensors_interval_ms, 1000, 120_000),
            process_interval_ms: c(self.process_interval_ms, 2000, 120_000),
            persist_interval_s: c(self.persist_interval_s, 5, 300),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MenuBarSettings {
    pub enabled: bool,
    pub show_cpu: bool,
    pub show_ram: bool,
    pub show_tokens: bool,
    pub show_status: bool,
}
impl Default for MenuBarSettings {
    fn default() -> Self {
        Self { enabled: true, show_cpu: true, show_ram: true, show_tokens: true, show_status: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetSettings {
    pub enabled: bool,
    pub mode: String, // "compact" | "expanded"
    pub opacity: f32,
}
impl Default for WidgetSettings {
    fn default() -> Self {
        Self { enabled: false, mode: "compact".into(), opacity: 0.92 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HistorySettings {
    pub system_retention_days: u32,
    /// 0 = keep forever
    pub ai_retention_days: u32,
    pub alert_retention_days: u32,
}
impl Default for HistorySettings {
    fn default() -> Self {
        Self { system_retention_days: 7, ai_retention_days: 0, alert_retention_days: 90 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    /// Local telemetry proxy (127.0.0.1 only).
    pub proxy_enabled: bool,
    pub proxy_port: u16,
    /// "auto" = best available per day; or force a single source.
    pub usage_source: String, // "auto" | "provider_reported" | "telemetry" | "imported"
}
impl Default for AiSettings {
    fn default() -> Self {
        Self { proxy_enabled: false, proxy_port: 47821, usage_source: "auto".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub monitoring: MonitoringSettings,
    pub menu_bar: MenuBarSettings,
    pub widget: WidgetSettings,
    pub history: HistorySettings,
    pub ai: AiSettings,
}

const KEY: &str = "settings";

impl Settings {
    pub fn validate(&self) -> AppResult<()> {
        if !["en", "vi"].contains(&self.general.language.as_str()) {
            return Err(AppError::Invalid("language must be 'en' or 'vi'".into()));
        }
        if !["compact", "expanded"].contains(&self.widget.mode.as_str()) {
            return Err(AppError::Invalid("widget mode must be compact or expanded".into()));
        }
        if self.ai.proxy_port < 1024 {
            return Err(AppError::Invalid("proxy port must be ≥ 1024".into()));
        }
        if !["auto", "provider_reported", "telemetry", "imported"].contains(&self.ai.usage_source.as_str()) {
            return Err(AppError::Invalid("unknown usage source".into()));
        }
        Ok(())
    }

    pub fn load(db: &Db) -> AppResult<Settings> {
        db.call_sync(|c| {
            let v: Option<String> = c
                .query_row("SELECT value_json FROM application_settings WHERE key = ?1", [KEY], |r| r.get(0))
                .ok();
            Ok(match v {
                Some(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                    tracing::warn!("settings document invalid, using defaults: {e}");
                    Settings::default()
                }),
                None => Settings::default(),
            })
        })
    }

    pub async fn save(&self, db: &Db) -> AppResult<()> {
        self.validate()?;
        let json = serde_json::to_string(self)?;
        db.call(move |c| {
            c.execute(
                "INSERT INTO application_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                rusqlite::params![KEY, json, crate::util::now_ms()],
            )?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_uses_defaults() {
        let s: Settings = serde_json::from_str(r#"{"general":{"language":"vi"}}"#).unwrap();
        assert_eq!(s.general.language, "vi");
        assert_eq!(s.monitoring.cpu_interval_ms, 2000);
    }

    #[test]
    fn sanitize_clamps() {
        let m = MonitoringSettings { cpu_interval_ms: 1, ..Default::default() }.sanitized();
        assert_eq!(m.cpu_interval_ms, 500);
    }

    #[tokio::test]
    async fn roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let mut s = Settings::default();
        s.appearance.accent = "purple".into();
        s.save(&db).await.unwrap();
        assert_eq!(Settings::load(&db).unwrap().appearance.accent, "purple");
    }
}
