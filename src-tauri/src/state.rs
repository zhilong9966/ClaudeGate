use crate::config::{AppConfig, AppProfile};
use crate::guard::AppGuard;
use crate::rules::Report;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub config_path: PathBuf,
    pub cfg: Mutex<AppConfig>,
    /// 每个应用最近一次检测报告
    pub reports: Mutex<HashMap<String, Report>>,
    /// 每个正在守护的应用的状态（key = 应用 id）
    pub guards: Mutex<HashMap<String, AppGuard>>,
}

impl AppState {
    pub fn config(&self) -> AppConfig {
        self.cfg.lock().unwrap().clone()
    }

    pub fn app(&self, id: &str) -> Result<AppProfile, String> {
        self.cfg.lock().unwrap().app(id).cloned().ok_or_else(|| format!("未知应用: {id}"))
    }
}
