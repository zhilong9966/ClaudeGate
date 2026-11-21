mod commands;
mod config;
mod detect;
mod guard;
mod http;
mod launcher;
mod logging;
mod process;
mod rules;
mod shortcut;
mod state;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::Manager;

/// 启动时解析出的默认目录：(日志目录, 离线库目录)
static DEFAULT_DIRS: OnceLock<(String, String)> = OnceLock::new();

pub(crate) fn default_dirs() -> (Option<String>, Option<String>) {
    match DEFAULT_DIRS.get() {
        Some((l, d)) => (Some(l.clone()), Some(d.clone())),
        None => (None, None),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let config_path = dir.join("config.json");
            let log_dir = app.path().app_log_dir()?.to_string_lossy().into_owned();
            let db_dir = app.path().app_data_dir()?.join("geo").to_string_lossy().into_owned();
            let _ = DEFAULT_DIRS.set((log_dir.clone(), db_dir.clone()));

            let mut cfg = config::AppConfig::load(&config_path);
            if cfg.general.log.dir.trim().is_empty() {
                cfg.general.log.dir = log_dir;
            }
            if cfg.network.local_db.dir.trim().is_empty() {
                cfg.network.local_db.dir = db_dir;
            }
            let _ = cfg.save(&config_path);

            logging::init(&cfg.general.log);
            log::info!("===== ClaudeGate {} 启动（{} / {}） =====", app.package_info().version, std::env::consts::OS, std::env::consts::ARCH);
            log::info!("配置文件 {}", config_path.display());
            log::info!("日志目录 {}，级别 {}，保留 {} 天", cfg.general.log.dir, cfg.general.log.level, cfg.general.log.retention_days);
            log::info!("IP 数据源 {}，离线库目录 {}", cfg.network.risk_provider, cfg.network.local_db.dir);
            for a in &cfg.apps {
                log::info!("应用 {}（{}）启用: {}，出口方式 {}", a.name, a.id, a.enabled, a.egress.mode);
            }

            // 后台预先下载/更新离线库，首次检测时就不用等
            if !matches!(cfg.network.risk_provider.as_str(), "ipnetcoffee" | "ipapi_is") {
                let (net, db) = (cfg.network.clone(), cfg.network.local_db.clone());
                tauri::async_runtime::spawn(async move {
                    if let Ok(c) = http::client(&net) {
                        detect::localdb::update(&c, &db, false).await;
                    }
                });
            }
            app.manage(state::AppState {
                config_path,
                cfg: Mutex::new(cfg),
                reports: Mutex::new(HashMap::new()),
                guard_gen: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::default_config,
            commands::config_path,
            commands::run_checks,
            commands::launch_app,
            commands::kill_app,
            commands::list_app_processes,
            commands::geo_db_status,
            commands::update_geo_db,
            commands::shortcut_info,
            commands::create_shortcut,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ClaudeGate")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let cfg = app.state::<state::AppState>().config();
                log::info!("ClaudeGate 退出（退出时关闭应用: {}）", cfg.general.kill_apps_on_exit);
                if cfg.general.kill_apps_on_exit {
                    for a in cfg.apps.iter().filter(|a| a.enabled) {
                        tauri::async_runtime::block_on(process::kill_all(&a.process));
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    /// 真实网络检测（只读，不启动/不结束任何进程）：cargo test --lib -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_checks() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.network.local_db.dir = std::env::temp_dir().join("claudegate-test-geo").to_string_lossy().into_owned();
        for app in &cfg.apps {
            let facts = crate::detect::collect(&cfg, app).await;
            let report = crate::rules::evaluate(app, facts);
            println!("===== {} =====", app.name);
            for c in &report.checks {
                println!("{:?}\t{}{}\t{}", c.status, if c.required { "★" } else { " " }, c.title, c.message);
            }
            println!("can_launch = {}", report.can_launch);
            let procs: Vec<String> = crate::process::list(&app.process).into_iter().map(|p| p.name).collect();
            println!("matched processes (list only): {procs:?}");
        }
    }
}
