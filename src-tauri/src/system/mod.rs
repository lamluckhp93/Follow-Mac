//! Real-time system monitoring. Every value is optional: `None` means the
//! value could not be read, and each section carries a `status` explaining why.

pub mod battery;
pub mod counters;
pub mod cpu;
pub mod disk;
pub mod fan;
pub mod gpu;
pub mod history;
#[cfg(target_os = "macos")]
pub mod iokit;
pub mod macos;
pub mod memory;
pub mod network;
pub mod sampler;
pub mod temperature;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    #[default]
    Ok,
    /// Temporarily unavailable (read failed); `error` explains.
    Unavailable,
    /// No API available for this metric on this machine.
    Unsupported,
    /// Hardware is absent (e.g. no battery on a Mac mini, no fan on a MacBook Air).
    NotPresent,
    /// Not sampled yet.
    Pending,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CpuMetrics {
    pub status: MetricStatus,
    pub error: Option<String>,
    pub usage: Option<f32>,
    pub user: Option<f32>,
    pub system: Option<f32>,
    pub idle: Option<f32>,
    pub per_core: Vec<f32>,
    pub load_avg: Option<[f64; 3]>,
    pub top_processes: Vec<ProcessInfo>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMetrics {
    pub status: MetricStatus,
    pub error: Option<String>,
    pub total: Option<u64>,
    pub used: Option<u64>,
    pub available: Option<u64>,
    pub app: Option<u64>,
    pub wired: Option<u64>,
    pub compressed: Option<u64>,
    pub cached_files: Option<u64>,
    pub used_percent: Option<f32>,
    /// kern.memorystatus_vm_pressure_level: 1 normal, 2 warning, 4 critical
    pub pressure_level: Option<u32>,
    pub pressure_label: Option<String>,
    pub swap_total: Option<u64>,
    pub swap_used: Option<u64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuMetrics {
    pub status: MetricStatus,
    pub error: Option<String>,
    pub model: Option<String>,
    pub cores: Option<u32>,
    pub usage: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub unified_memory: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiskMetrics {
    pub status: MetricStatus,
    pub error: Option<String>,
    pub name: Option<String>,
    pub mount_point: Option<String>,
    pub file_system: Option<String>,
    pub total: Option<u64>,
    pub used: Option<u64>,
    pub free: Option<u64>,
    pub used_percent: Option<f32>,
    pub io_status: MetricStatus,
    pub read_bps: Option<f64>,
    pub write_bps: Option<f64>,
    pub total_read: Option<u64>,
    pub total_written: Option<u64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BatteryMetrics {
    pub status: MetricStatus,
    pub error: Option<String>,
    pub percent: Option<f32>,
    pub charging: Option<bool>,
    pub plugged_in: Option<bool>,
    pub fully_charged: Option<bool>,
    pub cycle_count: Option<u32>,
    /// Current full-charge capacity / design capacity.
    pub health_percent: Option<f32>,
    pub design_capacity_mah: Option<u32>,
    pub full_charge_capacity_mah: Option<u32>,
    pub time_remaining_min: Option<u32>,
    pub temperature_c: Option<f32>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetworkMetrics {
    pub status: MetricStatus,
    pub error: Option<String>,
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
    pub total_rx: Option<u64>,
    pub total_tx: Option<u64>,
    pub primary_interface: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SensorReading {
    pub label: String,
    pub celsius: f32,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TemperatureMetrics {
    pub status: MetricStatus,
    pub error: Option<String>,
    pub cpu_c: Option<f32>,
    pub gpu_c: Option<f32>,
    pub battery_c: Option<f32>,
    pub sensors: Vec<SensorReading>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FanMetrics {
    pub status: MetricStatus,
    pub note: Option<String>,
    pub rpm: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub ts: i64,
    pub monitoring: bool,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub gpu: GpuMetrics,
    pub disk: DiskMetrics,
    pub battery: BatteryMetrics,
    pub network: NetworkMetrics,
    pub temperature: TemperatureMetrics,
    pub fan: FanMetrics,
}

/// Static device information shown in the header.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub device_name: Option<String>,
    pub model_identifier: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub os_build: Option<String>,
    pub kernel_version: Option<String>,
    pub chip: Option<String>,
    pub architecture: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: Option<usize>,
    pub performance_cores: Option<u32>,
    pub efficiency_cores: Option<u32>,
    pub total_memory: Option<u64>,
    pub boot_time: Option<u64>,
    pub fanless: Option<bool>,
}
