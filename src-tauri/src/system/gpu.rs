use super::{GpuMetrics, MetricStatus};
use crate::util::now_ms;

/// GPU statistics from the `IOAccelerator` service's `PerformanceStatistics`
/// dictionary (the same source Activity Monitor's GPU History uses).
/// Apple Silicon GPUs use unified memory, so "memory" is the system memory
/// the GPU driver currently has in use; there is no dedicated VRAM total.
#[cfg(target_os = "macos")]
pub fn collect() -> GpuMetrics {
    let mut m = GpuMetrics { updated_at: now_ms(), ..Default::default() };
    let Some(p) = super::iokit::service_properties("IOAccelerator") else {
        m.status = MetricStatus::Unsupported;
        m.error = Some("no IOAccelerator service found".into());
        return m;
    };
    m.model = p.string("model");
    m.cores = p.i64("gpu-core-count").map(|v| v as u32);
    m.unified_memory = crate::system::macos::sysctl_string("machdep.cpu.brand_string")
        .map(|b| b.starts_with("Apple"))
        .unwrap_or(false);

    match p.dict("PerformanceStatistics") {
        Some(stats) => {
            m.usage = stats.i64("Device Utilization %").map(|v| v as f32);
            m.memory_used = stats.i64("In use system memory").map(|v| v as u64);
            if !m.unified_memory {
                m.memory_total = p.i64("VRAM,totalMB").map(|mb| mb as u64 * 1024 * 1024);
            }
            m.status = if m.usage.is_some() { MetricStatus::Ok } else { MetricStatus::Unsupported };
            if m.usage.is_none() {
                m.error = Some("driver does not report utilisation".into());
            }
        }
        None => {
            m.status = MetricStatus::Unsupported;
            m.error = Some("driver exposes no PerformanceStatistics".into());
        }
    }
    m
}

#[cfg(not(target_os = "macos"))]
pub fn collect() -> GpuMetrics {
    GpuMetrics { status: MetricStatus::Unsupported, updated_at: now_ms(), ..Default::default() }
}
