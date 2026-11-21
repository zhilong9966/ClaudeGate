mod commands;
mod config;
mod detect;
mod guard;
mod http;
mod launcher;
mod process;
mod rules;
mod state;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let config_path = dir.join("config.json");
            let cfg = config::AppConfig::load(&config_path);
            let _ = cfg.save(&config_path);
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building ClaudeGate")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let cfg = app.state::<state::AppState>().config();
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
        let cfg = crate::config::AppConfig::default();
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
