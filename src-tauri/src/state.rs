use crate::config::AppConfig;
use crate::rules::Report;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;

pub struct AppState {
    pub config_path: PathBuf,
    pub cfg: Mutex<AppConfig>,
    pub last_report: Mutex<Option<Report>>,
    /// 守护任务代号，自增即可让旧守护退出
    pub guard_gen: AtomicU64,
}

impl AppState {
    pub fn config(&self) -> AppConfig {
        self.cfg.lock().unwrap().clone()
    }
}
