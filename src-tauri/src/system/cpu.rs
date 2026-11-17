use super::{macos, CpuMetrics, MetricStatus, ProcessInfo};
use crate::util::now_ms;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// Computes CPU usage. Total and per-core come from sysinfo; the user/system/idle
/// split comes from Mach `host_statistics(HOST_CPU_LOAD_INFO)` tick deltas.
#[derive(Default)]
pub struct CpuCollector {
    prev_ticks: Option<[u64; 4]>,
    top: Vec<ProcessInfo>,
}

impl CpuCollector {
    pub fn collect(&mut self, sys: &mut System) -> CpuMetrics {
        sys.refresh_cpu_usage();
        let per_core: Vec<f32> = sys.cpus().iter().map(|c| c.usage()).collect();
        let mut m = CpuMetrics {
            per_core,
            usage: Some(sys.global_cpu_usage()),
            updated_at: now_ms(),
            ..Default::default()
        };

        match macos::host_cpu_ticks() {
            Some(now) => {
                if let Some(prev) = self.prev_ticks {
                    let d: Vec<u64> = now.iter().zip(prev.iter()).map(|(a, b)| a.saturating_sub(*b)).collect();
                    let total = (d[0] + d[1] + d[2] + d[3]) as f32;
                    if total > 0.0 {
                        m.user = Some((d[0] + d[3]) as f32 / total * 100.0);
                        m.system = Some(d[1] as f32 / total * 100.0);
                        m.idle = Some(d[2] as f32 / total * 100.0);
                        // Prefer the tick-based total so user+system+idle add up.
                        m.usage = Some(100.0 - d[2] as f32 / total * 100.0);
                    }
                }
                self.prev_ticks = Some(now);
            }
            None => m.error = Some("host_statistics(HOST_CPU_LOAD_INFO) failed".into()),
        }

        m.load_avg = System::load_average().ok().map(|l| [l.one, l.five, l.fifteen]);
        m.top_processes = self.top.clone();
        m.status = if m.usage.is_some() { MetricStatus::Ok } else { MetricStatus::Unavailable };
        m
    }

    /// Refresh the process table (heavier; runs on its own interval).
    pub fn refresh_processes(&mut self, sys: &mut System, limit: usize) {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let mut procs: Vec<ProcessInfo> = sys
            .processes()
            .values()
            .map(|p| ProcessInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                cpu: p.cpu_usage(),
                memory: p.memory(),
            })
            .collect();
        procs.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
        procs.truncate(limit);
        self.top = procs;
    }
}
