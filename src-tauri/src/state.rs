use crate::ai::proxy::ProxyHandle;
use crate::ai::AiService;
use crate::alerts::AlertEngine;
use crate::database::Db;
use crate::settings::Settings;
use crate::system::sampler::SharedBuffers;
use crate::system::{DeviceInfo, SystemSnapshot};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub db: Db,
    pub settings: Arc<RwLock<Settings>>,
    pub buffers: SharedBuffers,
    pub snapshot: watch::Receiver<Arc<SystemSnapshot>>,
    pub device: DeviceInfo,
    pub ai: AiService,
    pub alerts: AlertEngine,
    pub cancel: CancellationToken,
    pub proxy: tokio::sync::Mutex<Option<ProxyHandle>>,
    pub proxy_error: Mutex<Option<String>>,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub sampler_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings.read().map(|s| s.clone()).unwrap_or_default()
    }
}
