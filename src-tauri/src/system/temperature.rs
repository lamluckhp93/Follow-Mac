use super::{MetricStatus, SensorReading, TemperatureMetrics};
use crate::util::now_ms;
use sysinfo::Components;

/// Temperatures from IOHIDEventSystem sensors (via sysinfo `Components`).
/// On Apple Silicon these are labelled e.g. "PMU tdie1", "PMU tdev1", "gas gauge battery".
/// CPU = mean of die sensors ("tdie"), GPU is not separately labelled on most
/// Apple Silicon machines, so it stays `None` unless a GPU-labelled sensor exists.
pub struct TemperatureCollector {
    components: Option<Components>,
}

impl Default for TemperatureCollector {
    fn default() -> Self {
        Self { components: Components::new_with_refreshed_list().ok() }
    }
}

pub fn summarise(readings: &[SensorReading]) -> (Option<f32>, Option<f32>, Option<f32>) {
    let avg = |f: &dyn Fn(&str) -> bool| {
        let v: Vec<f32> = readings.iter().filter(|r| f(&r.label.to_lowercase())).map(|r| r.celsius).collect();
        (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
    };
    let cpu = avg(&|l| l.contains("tdie") || l.contains("cpu") || l.contains("pacc") || l.contains("eacc"));
    let gpu = avg(&|l| l.contains("gpu"));
    let battery = avg(&|l| l.contains("battery"));
    (cpu, gpu, battery)
}

impl TemperatureCollector {
    pub fn collect(&mut self) -> TemperatureMetrics {
        let mut m = TemperatureMetrics { updated_at: now_ms(), ..Default::default() };
        let Some(c) = self.components.as_mut() else {
            m.status = MetricStatus::Unsupported;
            m.error = Some("temperature sensors not accessible".into());
            return m;
        };
        c.refresh(false);
        m.sensors = c
            .iter()
            .filter_map(|s| {
                let t = s.temperature()?;
                // Discard obviously invalid readings.
                (t.is_finite() && t > 0.0 && t < 150.0).then(|| SensorReading { label: s.label().to_string(), celsius: t })
            })
            .collect();
        if m.sensors.is_empty() {
            m.status = MetricStatus::Unsupported;
            m.error = Some("no readable temperature sensors".into());
            return m;
        }
        let (cpu, gpu, battery) = summarise(&m.sensors);
        m.cpu_c = cpu;
        m.gpu_c = gpu;
        m.battery_c = battery;
        m.status = MetricStatus::Ok;
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summarises_labels() {
        let r = vec![
            SensorReading { label: "PMU tdie1".into(), celsius: 40.0 },
            SensorReading { label: "PMU tdie2".into(), celsius: 50.0 },
            SensorReading { label: "gas gauge battery".into(), celsius: 30.0 },
        ];
        let (c, g, b) = summarise(&r);
        assert_eq!(c, Some(45.0));
        assert_eq!(g, None);
        assert_eq!(b, Some(30.0));
    }
}
