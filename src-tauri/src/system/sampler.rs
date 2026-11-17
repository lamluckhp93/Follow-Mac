//! The sampling loop. Runs on a dedicated OS thread (sysinfo/IOKit calls are
//! synchronous) and publishes snapshots through a `tokio::sync::watch`
//! channel. Each collector has its own interval; persistence is batched.

use super::cpu::CpuCollector;
use super::disk::DiskCollector;
use super::history::{self, MetricPoint};
use super::network::NetworkCollector;
use super::temperature::TemperatureCollector;
use super::{battery, fan, gpu, macos, memory, DeviceInfo, MetricStatus, SystemSnapshot};
use crate::database::Db;
use crate::settings::Settings;
use crate::util::now_ms;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use sysinfo::System;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub const RING_WINDOW_MS: i64 = 60 * 60 * 1000;
const FLUSH_EVERY: Duration = Duration::from_secs(60);
const TOP_PROCESSES: usize = 8;

#[derive(Clone, Default)]
pub struct SharedBuffers {
    /// Sample-resolution history for the last hour (live charts).
    pub ring: Arc<Mutex<VecDeque<MetricPoint>>>,
    /// Downsampled rows waiting to be written to SQLite.
    pub pending: Arc<Mutex<Vec<MetricPoint>>>,
}

pub fn device_info(sys: Option<&System>) -> DeviceInfo {
    let model = macos::sysctl_string("hw.model");
    DeviceInfo {
        device_name: computer_name().or_else(|| System::host_name().ok()),
        fanless: model.as_deref().and_then(macos::is_fanless_model),
        model_identifier: model,
        // sysinfo reports the kernel name ("Darwin"); show the product name instead.
        os_name: if cfg!(target_os = "macos") { Some("macOS".to_string()) } else { System::name().ok() },
        os_version: System::os_version().ok(),
        os_build: macos::sysctl_string("kern.osversion"),
        kernel_version: System::kernel_version().ok(),
        chip: macos::sysctl_string("machdep.cpu.brand_string"),
        architecture: std::env::consts::ARCH.to_string(),
        physical_cores: System::physical_core_count().ok(),
        logical_cores: sys.map(|s| s.cpus().len()).filter(|n| *n > 0),
        performance_cores: macos::sysctl_u32("hw.perflevel0.physicalcpu"),
        efficiency_cores: macos::sysctl_u32("hw.perflevel1.physicalcpu"),
        total_memory: macos::sysctl_u64("hw.memsize"),
        boot_time: System::boot_time().ok(),
    }
}

/// The user-visible computer name ("Lam's MacBook Air"). Read once at startup.
fn computer_name() -> Option<String> {
    let out = std::process::Command::new("/usr/sbin/scutil").arg("--get").arg("ComputerName").output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

struct Due {
    last: Option<Instant>,
}
impl Due {
    fn check(&mut self, now: Instant, every: Duration) -> bool {
        match self.last {
            Some(t) if now.duration_since(t) < every => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

pub struct Sampler {
    pub db: Db,
    pub settings: Arc<RwLock<Settings>>,
    pub tx: watch::Sender<Arc<SystemSnapshot>>,
    pub buffers: SharedBuffers,
    pub cancel: CancellationToken,
    pub fanless: Option<bool>,
}

impl Sampler {
    pub fn spawn(self) -> std::thread::JoinHandle<()> {
        std::thread::Builder::new()
            .name("system-sampler".into())
            .spawn(move || self.run())
            .expect("spawn sampler thread")
    }

    fn run(self) {
        let mut sys = match System::new() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("system sampler cannot start: {e}");
                return;
            }
        };
        let mut cpu = CpuCollector::default();
        let mut disk = DiskCollector::default();
        let mut net = NetworkCollector::default();
        let mut temps = TemperatureCollector::default();
        let mut snap = SystemSnapshot { fan: fan::collect(self.fanless), ..Default::default() };
        for status in [
            &mut snap.cpu.status,
            &mut snap.memory.status,
            &mut snap.gpu.status,
            &mut snap.disk.status,
            &mut snap.battery.status,
            &mut snap.network.status,
            &mut snap.temperature.status,
        ] {
            *status = MetricStatus::Pending;
        }

        let (mut d_cpu, mut d_mem, mut d_net, mut d_disk, mut d_space, mut d_bat, mut d_sens, mut d_proc) = (
            Due { last: None },
            Due { last: None },
            Due { last: None },
            Due { last: None },
            Due { last: None },
            Due { last: None },
            Due { last: None },
            Due { last: None },
        );
        let mut accum: Vec<MetricPoint> = Vec::new();
        let mut last_persist = Instant::now();
        let mut last_flush = Instant::now();
        let mut was_enabled = true;

        tracing::info!("system sampler started");
        while !self.cancel.is_cancelled() {
            let cfg = self.settings.read().map(|s| s.monitoring.sanitized()).unwrap_or_default();
            if !cfg.enabled {
                if was_enabled {
                    snap.monitoring = false;
                    snap.ts = now_ms();
                    let _ = self.tx.send(Arc::new(snap.clone()));
                    was_enabled = false;
                }
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            was_enabled = true;
            snap.monitoring = true;

            let now = Instant::now();
            let ms = Duration::from_millis;
            if d_proc.check(now, ms(cfg.process_interval_ms)) {
                cpu.refresh_processes(&mut sys, TOP_PROCESSES);
            }
            if d_cpu.check(now, ms(cfg.cpu_interval_ms)) {
                snap.cpu = cpu.collect(&mut sys);
                snap.gpu = gpu::collect();
            }
            if d_mem.check(now, ms(cfg.memory_interval_ms)) {
                snap.memory = memory::collect(&mut sys);
            }
            if d_net.check(now, ms(cfg.network_interval_ms)) {
                snap.network = net.collect();
            }
            if d_disk.check(now, ms(cfg.disk_interval_ms)) {
                let space = d_space.check(now, Duration::from_secs(60));
                let prev = snap.disk.clone();
                let mut d = disk.collect(space);
                if !space && prev.total.is_some() {
                    // Keep last known capacity between storage refreshes.
                    d.total = prev.total;
                    d.used = prev.used;
                    d.free = prev.free;
                    d.used_percent = prev.used_percent;
                }
                snap.disk = d;
            }
            if d_bat.check(now, ms(cfg.battery_interval_ms)) {
                snap.battery = battery::collect();
            }
            if d_sens.check(now, ms(cfg.sensors_interval_ms)) {
                snap.temperature = temps.collect();
            }
            snap.ts = now_ms();

            let point = MetricPoint::from_snapshot(&snap);
            if let Ok(mut ring) = self.buffers.ring.lock() {
                ring.push_back(point.clone());
                let cutoff = snap.ts - RING_WINDOW_MS;
                while ring.front().is_some_and(|p| p.ts < cutoff) {
                    ring.pop_front();
                }
            }
            accum.push(point);
            let _ = self.tx.send(Arc::new(snap.clone()));

            if last_persist.elapsed() >= Duration::from_secs(cfg.persist_interval_s) && !accum.is_empty() {
                let row = history::average(&accum, now_ms());
                accum.clear();
                last_persist = Instant::now();
                if let Ok(mut p) = self.buffers.pending.lock() {
                    p.push(row);
                }
            }
            if last_flush.elapsed() >= FLUSH_EVERY {
                self.flush();
                last_flush = Instant::now();
            }

            // Sleep until the next collector is due (bounded, cancel-aware).
            let tick = [cfg.cpu_interval_ms, cfg.memory_interval_ms, cfg.network_interval_ms, cfg.disk_interval_ms]
                .into_iter()
                .min()
                .unwrap_or(2000)
                .max(250);
            let deadline = now + Duration::from_millis(tick);
            while Instant::now() < deadline && !self.cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())));
            }
        }
        // Graceful shutdown: persist whatever is buffered.
        if !accum.is_empty() {
            if let Ok(mut p) = self.buffers.pending.lock() {
                p.push(history::average(&accum, now_ms()));
            }
        }
        self.flush();
        tracing::info!("system sampler stopped");
    }

    fn flush(&self) {
        let rows: Vec<MetricPoint> = match self.buffers.pending.lock() {
            Ok(mut p) => std::mem::take(&mut *p),
            Err(_) => return,
        };
        if rows.is_empty() {
            return;
        }
        if let Err(e) = history::insert_batch(&self.db, &rows) {
            tracing::error!("failed to persist {} metric rows: {e}", rows.len());
            // Put them back so they are retried on the next flush.
            if let Ok(mut p) = self.buffers.pending.lock() {
                let mut rows = rows;
                rows.append(&mut p);
                *p = rows;
                if p.len() > 10_000 {
                    let excess = p.len() - 10_000;
                    p.drain(..excess);
                }
            }
        }
    }
}
