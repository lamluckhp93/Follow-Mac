//! Integration tests against the real hardware of the machine running them.
//! They verify that every collector produces plausible, non-simulated values.
//!
//! Set `QA_OUT=/some/dir` to also write the collected snapshot and device info
//! as JSON (used for visual QA of the UI with real data).

use mac_observer_lib::system::cpu::CpuCollector;
use mac_observer_lib::system::disk::DiskCollector;
use mac_observer_lib::system::network::NetworkCollector;
use mac_observer_lib::system::temperature::TemperatureCollector;
use mac_observer_lib::system::{battery, fan, gpu, memory, sampler, MetricStatus, SystemSnapshot};
use std::time::Duration;
use sysinfo::System;

#[test]
fn collectors_return_real_values() {
    let mut sys = System::new().expect("sysinfo");
    let mut cpu = CpuCollector::default();
    let mut disk = DiskCollector::default();
    let mut net = NetworkCollector::default();
    let mut temps = TemperatureCollector::default();
    let device = sampler::device_info(Some(&sys));

    // Rates need two samples.
    cpu.refresh_processes(&mut sys, 8);
    let _ = cpu.collect(&mut sys);
    let _ = disk.collect(true);
    let _ = net.collect();
    std::thread::sleep(Duration::from_millis(1200));
    cpu.refresh_processes(&mut sys, 8);

    let snap = SystemSnapshot {
        ts: chrono::Utc::now().timestamp_millis(),
        monitoring: true,
        cpu: cpu.collect(&mut sys),
        memory: memory::collect(&mut sys),
        gpu: gpu::collect(),
        disk: disk.collect(true),
        battery: battery::collect(),
        network: net.collect(),
        temperature: temps.collect(),
        fan: fan::collect(device.fanless),
    };

    // CPU
    let u = snap.cpu.usage.expect("cpu usage");
    assert!((0.0..=100.0).contains(&u), "cpu usage {u}");
    let parts = snap.cpu.user.unwrap() + snap.cpu.system.unwrap() + snap.cpu.idle.unwrap();
    assert!((parts - 100.0).abs() < 0.5, "user+system+idle = {parts}");
    assert!(!snap.cpu.per_core.is_empty());
    assert!(!snap.cpu.top_processes.is_empty());

    // Memory
    let total = snap.memory.total.expect("mem total");
    let used = snap.memory.used.expect("mem used");
    assert!(used > 0 && used <= total);
    assert!(matches!(snap.memory.pressure_level, Some(1 | 2 | 4)));

    // Disk
    assert_eq!(snap.disk.status, MetricStatus::Ok);
    assert!(snap.disk.total.unwrap() > snap.disk.free.unwrap());
    assert_eq!(snap.disk.io_status, MetricStatus::Ok);
    assert!(snap.disk.read_bps.unwrap() < 50e9, "implausible disk rate");

    // Network
    assert_eq!(snap.network.status, MetricStatus::Ok);
    assert!(snap.network.rx_bps.unwrap() < 20e9, "implausible network rate");

    // Battery: either a real reading or explicitly absent.
    match snap.battery.status {
        MetricStatus::Ok => {
            let p = snap.battery.percent.unwrap();
            assert!((0.0..=100.0).contains(&p));
        }
        MetricStatus::NotPresent => {}
        s => panic!("unexpected battery status {s:?}"),
    }

    // GPU on Apple Silicon reports utilisation.
    if device.chip.as_deref().is_some_and(|c| c.starts_with("Apple")) {
        assert_eq!(snap.gpu.status, MetricStatus::Ok);
        assert!(snap.gpu.model.is_some());
    }

    // Fan must never be faked.
    assert!(snap.fan.rpm.is_empty());
    assert!(matches!(snap.fan.status, MetricStatus::NotPresent | MetricStatus::Unsupported));

    if let Ok(dir) = std::env::var("QA_OUT") {
        std::fs::write(format!("{dir}/snapshot.json"), serde_json::to_string_pretty(&snap).unwrap()).unwrap();
        std::fs::write(format!("{dir}/device.json"), serde_json::to_string_pretty(&device).unwrap()).unwrap();
    }
}
