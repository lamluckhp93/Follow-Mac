//! Alert evaluation. Conditions must hold for `duration_s` before firing; a
//! rule cannot fire again within `cooldown_s` (persisted, so restarts don't spam).

use super::notification::Notifier;
use crate::ai::usage::Totals;
use crate::database::repository::{self as repo, AlertEvent, AlertRule};
use crate::database::Db;
use crate::error::AppResult;
use crate::system::SystemSnapshot;
use crate::util::now_ms;
use crate::Emitter;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct EngineState {
    rules: Vec<AlertRule>,
    /// When each rule's condition started being true (ms).
    since: HashMap<String, i64>,
    last_fired: HashMap<String, i64>,
}

#[derive(Clone)]
pub struct AlertEngine {
    db: Db,
    notifier: Arc<dyn Notifier>,
    emit: Emitter,
    state: Arc<Mutex<EngineState>>,
}

pub struct Candidate {
    pub rule_id: &'static str,
    pub active: bool,
    pub severity: &'static str,
    pub title: String,
    pub message: String,
    pub value: Option<f64>,
}

/// Pure evaluation of system rules against a snapshot (unit-testable).
pub fn system_candidates(rules: &[AlertRule], s: &SystemSnapshot) -> Vec<Candidate> {
    let mut out = vec![];
    for r in rules.iter().filter(|r| r.enabled && r.category == "system") {
        let c = match r.id.as_str() {
            "cpu_high" => s.cpu.usage.map(|v| Candidate {
                rule_id: "cpu_high",
                active: v as f64 >= r.threshold,
                severity: "warning",
                title: "High CPU usage".into(),
                message: format!("CPU at {:.0}% (threshold {:.0}%)", v, r.threshold),
                value: Some(v as f64),
            }),
            "memory_high" => s.memory.used_percent.map(|v| Candidate {
                rule_id: "memory_high",
                active: v as f64 >= r.threshold,
                severity: "warning",
                title: "High memory usage".into(),
                message: format!("Memory at {:.0}% (threshold {:.0}%)", v, r.threshold),
                value: Some(v as f64),
            }),
            "memory_pressure" => s.memory.pressure_level.map(|lvl| Candidate {
                rule_id: "memory_pressure",
                active: lvl as f64 >= r.threshold,
                severity: if lvl >= 4 { "critical" } else { "warning" },
                title: "Memory pressure".into(),
                message: format!("macOS reports memory pressure: {}", crate::system::macos::pressure_label(lvl)),
                value: Some(lvl as f64),
            }),
            "battery_low" => match (s.battery.percent, s.battery.charging, s.battery.plugged_in) {
                (Some(p), charging, plugged) => Some(Candidate {
                    rule_id: "battery_low",
                    active: (p as f64) <= r.threshold && charging != Some(true) && plugged != Some(true),
                    severity: if p < 6.0 { "critical" } else { "warning" },
                    title: "Battery low".into(),
                    message: format!("Battery at {:.0}% and not charging", p),
                    value: Some(p as f64),
                }),
                _ => None,
            },
            "disk_full" => s.disk.used_percent.map(|v| Candidate {
                rule_id: "disk_full",
                active: v as f64 >= r.threshold,
                severity: if v >= 97.0 { "critical" } else { "warning" },
                title: "Disk almost full".into(),
                message: format!(
                    "Startup disk {:.0}% full ({} free)",
                    v,
                    s.disk.free.map(|f| format!("{:.1} GB", f as f64 / 1e9)).unwrap_or_else(|| "?".into())
                ),
                value: Some(v as f64),
            }),
            _ => None,
        };
        out.extend(c);
    }
    out
}

pub fn ai_candidates(rules: &[AlertRule], today: &Totals, month: &Totals) -> Vec<Candidate> {
    let mut out = vec![];
    for r in rules.iter().filter(|r| r.enabled && r.category == "ai") {
        let c = match r.id.as_str() {
            "tokens_daily" => Some(Candidate {
                rule_id: "tokens_daily",
                active: today.total as f64 >= r.threshold,
                severity: "warning",
                title: "Daily token usage exceeded".into(),
                message: format!("{} tokens today (threshold {})", today.total, r.threshold as i64),
                value: Some(today.total as f64),
            }),
            "cost_daily" => {
                let v = today.actual_cost + today.estimated_cost;
                Some(Candidate {
                    rule_id: "cost_daily",
                    active: v >= r.threshold,
                    severity: "warning",
                    title: "Daily AI budget exceeded".into(),
                    message: format!("${v:.2} spent today (budget ${:.2})", r.threshold),
                    value: Some(v),
                })
            }
            "cost_monthly" => {
                let v = month.actual_cost + month.estimated_cost;
                Some(Candidate {
                    rule_id: "cost_monthly",
                    active: v >= r.threshold,
                    severity: "critical",
                    title: "Monthly AI budget exceeded".into(),
                    message: format!("${v:.2} spent this month (budget ${:.2})", r.threshold),
                    value: Some(v),
                })
            }
            _ => None,
        };
        out.extend(c);
    }
    out
}

impl AlertEngine {
    pub fn new(db: Db, notifier: Arc<dyn Notifier>, emit: Emitter) -> Self {
        let e = Self { db, notifier, emit, state: Default::default() };
        if let Err(err) = e.reload_rules() {
            tracing::error!("failed to load alert rules: {err}");
        }
        e
    }

    pub fn reload_rules(&self) -> AppResult<()> {
        let (rules, last) = self.db.call_sync(|c| {
            let rules = repo::list_alert_rules(c)?;
            let mut last = HashMap::new();
            for r in &rules {
                if let Some(ts) = repo::last_alert_ts(c, &r.id)? {
                    last.insert(r.id.clone(), ts);
                }
            }
            Ok((rules, last))
        })?;
        let mut st = self.state.lock().unwrap();
        st.rules = rules;
        for (k, v) in last {
            st.last_fired.entry(k).or_insert(v);
        }
        Ok(())
    }

    fn rules(&self) -> Vec<AlertRule> {
        self.state.lock().unwrap().rules.clone()
    }

    /// Returns `Some(rule)` if the candidate should fire now.
    fn should_fire(&self, c: &Candidate, now: i64) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(rule) = st.rules.iter().find(|r| r.id == c.rule_id).cloned() else { return false };
        if !c.active {
            st.since.remove(c.rule_id);
            return false;
        }
        let since = *st.since.entry(c.rule_id.to_string()).or_insert(now);
        if now - since < rule.duration_s * 1000 {
            return false;
        }
        if let Some(last) = st.last_fired.get(c.rule_id) {
            if now - last < rule.cooldown_s * 1000 {
                return false;
            }
        }
        st.last_fired.insert(c.rule_id.to_string(), now);
        true
    }

    async fn fire(&self, rule_id: &str, severity: &str, title: String, message: String, value: Option<f64>) {
        let notified = self.notifier.notify(&title, &message);
        let mut ev = AlertEvent {
            id: 0,
            rule_id: rule_id.into(),
            severity: severity.into(),
            title,
            message,
            value,
            ts: now_ms(),
            notified,
        };
        let e2 = ev.clone();
        match self.db.call(move |c| repo::insert_alert_event(c, &e2)).await {
            Ok(id) => ev.id = id,
            Err(e) => tracing::error!("failed to store alert event: {e}"),
        }
        tracing::info!(rule = rule_id, "alert fired");
        (self.emit)("alerts://event", serde_json::to_value(&ev).unwrap_or_default());
    }

    pub async fn evaluate_system(&self, snap: &SystemSnapshot) {
        let now = now_ms();
        for c in system_candidates(&self.rules(), snap) {
            if self.should_fire(&c, now) {
                self.fire(c.rule_id, c.severity, c.title, c.message, c.value).await;
            }
        }
    }

    pub async fn evaluate_ai(&self, today: &Totals, month: &Totals) {
        let now = now_ms();
        for c in ai_candidates(&self.rules(), today, month) {
            if self.should_fire(&c, now) {
                self.fire(c.rule_id, c.severity, c.title, c.message, c.value).await;
            }
        }
    }

    /// Called when a provider sync fails. `auth` = credentials rejected.
    pub async fn provider_error(&self, provider: &str, message: &str, auth: bool) {
        let rule_id: &'static str = if auth { "auth_error" } else { "provider_error" };
        let candidate = Candidate {
            rule_id,
            active: true,
            severity: if auth { "critical" } else { "warning" },
            title: if auth { format!("{provider}: API key rejected") } else { format!("{provider}: sync failed") },
            message: message.to_string(),
            value: None,
        };
        let enabled = self.rules().iter().any(|r| r.id == rule_id && r.enabled);
        if enabled && self.should_fire(&candidate, now_ms()) {
            self.fire(rule_id, candidate.severity, candidate.title, candidate.message, None).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::{CpuMetrics, MemoryMetrics};

    struct NullNotifier;
    impl Notifier for NullNotifier {
        fn notify(&self, _: &str, _: &str) -> bool {
            true
        }
    }

    fn engine() -> AlertEngine {
        let db = Db::open_in_memory().unwrap();
        AlertEngine::new(db, Arc::new(NullNotifier), Arc::new(|_, _| {}))
    }

    fn snap(cpu: f32) -> SystemSnapshot {
        SystemSnapshot {
            cpu: CpuMetrics { usage: Some(cpu), ..Default::default() },
            memory: MemoryMetrics { used_percent: Some(10.0), pressure_level: Some(1), ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn candidates_respect_thresholds() {
        let e = engine();
        let c = system_candidates(&e.rules(), &snap(95.0));
        assert!(c.iter().any(|c| c.rule_id == "cpu_high" && c.active));
        assert!(c.iter().any(|c| c.rule_id == "memory_pressure" && !c.active));
        // AI rules are disabled by default
        assert!(ai_candidates(&e.rules(), &Totals::default(), &Totals::default()).is_empty());
    }

    #[test]
    fn duration_and_cooldown() {
        let e = engine();
        let c = Candidate { rule_id: "cpu_high", active: true, severity: "warning", title: "t".into(), message: "m".into(), value: None };
        // cpu_high has duration 60 s and cooldown 900 s by default
        assert!(!e.should_fire(&c, 1_000_000));
        assert!(!e.should_fire(&c, 1_030_000));
        assert!(e.should_fire(&c, 1_061_000));
        assert!(!e.should_fire(&c, 1_100_000)); // cooldown
        let off = Candidate { active: false, ..c };
        assert!(!e.should_fire(&off, 1_200_000));
    }

    #[tokio::test]
    async fn firing_persists_event() {
        let e = engine();
        e.provider_error("openai", "HTTP 500", false).await;
        let events = e.db.call(|c| repo::list_alert_events(c, 10)).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].rule_id, "provider_error");
        // cooldown prevents a second identical alert
        e.provider_error("openai", "HTTP 500", false).await;
        assert_eq!(e.db.call(|c| repo::list_alert_events(c, 10)).await.unwrap().len(), 1);
    }
}
