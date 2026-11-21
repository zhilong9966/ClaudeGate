use crate::config::{AppConfig, AppProfile};
use crate::rules::Report;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub config_path: PathBuf,
    pub cfg: Mutex<AppConfig>,
    /// 每个应用最近一次检测报告
    pub reports: Mutex<HashMap<String, Report>>,
    /// 每个应用的守护任务代号，自增即可让旧守护退出
    pub guard_gen: Mutex<HashMap<String, u64>>,
}

impl AppState {
    pub fn config(&self) -> AppConfig {
        self.cfg.lock().unwrap().clone()
    }

    pub fn app(&self, id: &str) -> Result<AppProfile, String> {
        self.cfg.lock().unwrap().app(id).cloned().ok_or_else(|| format!("未知应用: {id}"))
    }

    pub fn bump_guard(&self, id: &str) -> u64 {
        let mut g = self.guard_gen.lock().unwrap();
        let v = g.entry(id.to_string()).or_insert(0);
        *v += 1;
        *v
    }

    pub fn guard_gen(&self, id: &str) -> u64 {
        *self.guard_gen.lock().unwrap().get(id).unwrap_or(&0)
    }
}
