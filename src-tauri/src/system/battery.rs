use super::{BatteryMetrics, MetricStatus};
use crate::util::now_ms;

/// Reads the `AppleSmartBattery` IOKit service (no privileges needed).
///
/// Apple Silicon note: `CurrentCapacity`/`MaxCapacity` are percentages, while
/// `AppleRawMaxCapacity` / `DesignCapacity` are mAh. Health = raw max / design.
#[cfg(target_os = "macos")]
pub fn collect() -> BatteryMetrics {
    let mut m = BatteryMetrics { updated_at: now_ms(), ..Default::default() };
    let Some(p) = super::iokit::service_properties("AppleSmartBattery") else {
        m.status = MetricStatus::NotPresent;
        return m;
    };
    if p.bool("BatteryInstalled") == Some(false) {
        m.status = MetricStatus::NotPresent;
        return m;
    }

    let cur = p.i64("CurrentCapacity");
    let max = p.i64("MaxCapacity");
    m.percent = match (cur, max) {
        (Some(c), Some(mx)) if mx > 0 && mx <= 100 => Some(c as f32 / mx as f32 * 100.0),
        (Some(c), Some(mx)) if mx > 100 => Some(c as f32 / mx as f32 * 100.0), // Intel: mAh
        _ => None,
    };
    m.charging = p.bool("IsCharging");
    m.plugged_in = p.bool("ExternalConnected");
    m.fully_charged = p.bool("FullyCharged");
    m.cycle_count = p.i64("CycleCount").map(|v| v as u32);

    let design = p.i64("DesignCapacity").filter(|v| *v > 0);
    let raw_max = p.i64("AppleRawMaxCapacity").or_else(|| max.filter(|v| *v > 100)).filter(|v| *v > 0);
    m.design_capacity_mah = design.map(|v| v as u32);
    m.full_charge_capacity_mah = raw_max.map(|v| v as u32);
    if let (Some(d), Some(r)) = (design, raw_max) {
        m.health_percent = Some((r as f32 / d as f32 * 100.0).min(100.0));
    }

    // TimeRemaining is minutes; 65535 means "still calculating".
    m.time_remaining_min = p.i64("TimeRemaining").filter(|v| *v > 0 && *v < 65535).map(|v| v as u32);
    // Temperature is reported in centi-degrees Celsius.
    m.temperature_c = p.i64("Temperature").filter(|v| *v > 0).map(|v| v as f32 / 100.0);

    m.status = if m.percent.is_some() { MetricStatus::Ok } else { MetricStatus::Unavailable };
    if m.percent.is_none() {
        m.error = Some("battery capacity keys missing".into());
    }
    m
}

#[cfg(not(target_os = "macos"))]
pub fn collect() -> BatteryMetrics {
    BatteryMetrics { status: MetricStatus::Unsupported, updated_at: now_ms(), ..Default::default() }
}
