use super::{macos, MemoryMetrics, MetricStatus};
use crate::util::now_ms;
use sysinfo::System;

/// Memory breakdown matching Activity Monitor's definitions:
/// * App memory  = internal pages − purgeable pages
/// * Wired       = wired pages
/// * Compressed  = pages occupied by the compressor
/// * Used        = App + Wired + Compressed
/// * Cached files = external (file-backed) + purgeable pages
pub fn collect(sys: &mut System) -> MemoryMetrics {
    sys.refresh_memory();
    let total = sys.total_memory();
    let mut m = MemoryMetrics {
        total: (total > 0).then_some(total),
        swap_total: Some(sys.total_swap()),
        swap_used: Some(sys.used_swap()),
        updated_at: now_ms(),
        ..Default::default()
    };

    match macos::vm_pages() {
        Some(vm) => {
            let ps = vm.page_size;
            let app = vm.internal.saturating_sub(vm.purgeable) * ps;
            let wired = vm.wired * ps;
            let compressed = vm.compressed * ps;
            let used = app + wired + compressed;
            m.app = Some(app);
            m.wired = Some(wired);
            m.compressed = Some(compressed);
            m.cached_files = Some((vm.external + vm.purgeable) * ps);
            m.used = Some(used);
            if total > 0 {
                m.available = Some(total.saturating_sub(used));
                m.used_percent = Some(used as f32 / total as f32 * 100.0);
            }
        }
        None => {
            // Fall back to sysinfo's numbers.
            m.used = Some(sys.used_memory());
            m.available = Some(sys.available_memory());
            if total > 0 {
                m.used_percent = Some(sys.used_memory() as f32 / total as f32 * 100.0);
            }
            m.error = Some("host_statistics64 failed; using sysinfo values".into());
        }
    }

    match macos::sysctl_u32("kern.memorystatus_vm_pressure_level") {
        Some(level) => {
            m.pressure_level = Some(level);
            m.pressure_label = Some(macos::pressure_label(level).into());
        }
        None => {
            m.error.get_or_insert_with(|| "memory pressure level unavailable".into());
        }
    }

    m.status = if m.used.is_some() { MetricStatus::Ok } else { MetricStatus::Unavailable };
    m
}
