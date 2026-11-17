use super::counters::{CounterRates, Counters};
use super::{DiskMetrics, MetricStatus};
use crate::util::now_ms;
use std::path::Path;
use std::time::Instant;
use sysinfo::{DiskRefreshKind, Disks};

/// Boot volume capacity + aggregate read/write throughput.
///
/// On APFS the boot volume ("/") shares its container with the Data volume,
/// so capacity is taken from the volume mounted at `/System/Volumes/Data`
/// when present (this matches Finder / System Settings), otherwise `/`.
pub struct DiskCollector {
    disks: Disks,
    rates: CounterRates,
}

impl Default for DiskCollector {
    fn default() -> Self {
        Self {
            disks: Disks::new_with_refreshed_list().unwrap_or_else(|_| Disks::new().expect("disks")),
            rates: CounterRates::default(),
        }
    }
}

impl DiskCollector {
    pub fn collect(&mut self, refresh_space: bool) -> DiskMetrics {
        self.disks
            .refresh_specifics(true, if refresh_space { DiskRefreshKind::nothing().with_io_usage().with_storage() } else { DiskRefreshKind::nothing().with_io_usage() });
        let mut m = DiskMetrics { updated_at: now_ms(), ..Default::default() };

        let pick = self
            .disks
            .iter()
            .find(|d| d.mount_point() == Path::new("/System/Volumes/Data"))
            .or_else(|| self.disks.iter().find(|d| d.mount_point() == Path::new("/")));

        match pick {
            Some(d) => {
                let total = d.total_space();
                let free = d.available_space();
                m.name = Some(d.name().to_string_lossy().into_owned());
                m.mount_point = Some(d.mount_point().display().to_string());
                m.file_system = Some(d.file_system().to_string_lossy().into_owned());
                m.total = Some(total);
                m.free = Some(free);
                m.used = Some(total.saturating_sub(free));
                if total > 0 {
                    m.used_percent = Some(total.saturating_sub(free) as f32 / total as f32 * 100.0);
                }
                m.status = MetricStatus::Ok;
            }
            None => {
                m.status = MetricStatus::Unavailable;
                m.error = Some("boot volume not found".into());
            }
        }

        // I/O counters: sysinfo reads IOBlockStorageDriver statistics. Several
        // APFS volumes share one physical device and report identical
        // counters, so each distinct device is keyed once (by the smallest
        // volume name in its group) and rates are computed per device.
        let mut devices: std::collections::HashMap<(u64, u64), String> = std::collections::HashMap::new();
        for d in self.disks.iter() {
            let u = d.usage();
            if u.total_read_bytes == 0 && u.total_written_bytes == 0 {
                continue;
            }
            let name = d.name().to_string_lossy().into_owned();
            devices
                .entry((u.total_read_bytes, u.total_written_bytes))
                .and_modify(|n| {
                    if name < *n {
                        *n = name.clone()
                    }
                })
                .or_insert(name);
        }
        if devices.is_empty() {
            m.io_status = MetricStatus::Unavailable;
            return m;
        }
        m.total_read = Some(devices.keys().map(|k| k.0).sum());
        m.total_written = Some(devices.keys().map(|k| k.1).sum());
        let counters: Counters = devices.into_iter().map(|(k, name)| (name, k)).collect();
        match self.rates.update(Instant::now(), counters) {
            Some((r, w)) => {
                m.read_bps = Some(r);
                m.write_bps = Some(w);
                m.io_status = MetricStatus::Ok;
            }
            None => m.io_status = MetricStatus::Pending,
        }
        m
    }
}
