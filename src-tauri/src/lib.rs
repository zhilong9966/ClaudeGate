mod commands;
mod config;
mod detect;
mod guard;
mod http;
mod launcher;
mod process;
mod rules;
mod state;

use std::sync::atomic::AtomicU64;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let config_path = dir.join("config.json");
            let cfg = config::AppConfig::load(&config_path);
            let _ = cfg.save(&config_path);
            app.manage(state::AppState {
                config_path,
                cfg: Mutex::new(cfg),
                last_report: Mutex::new(None),
                guard_gen: AtomicU64::new(0),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::default_config,
            commands::config_path,
            commands::run_checks,
            commands::launch_claude,
            commands::kill_claude,
            commands::list_claude_processes,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ClaudeGate")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let st = app.state::<state::AppState>();
                let cfg = st.config();
                if cfg.general.kill_claude_on_exit {
                    tauri::async_runtime::block_on(process::kill_all(&cfg.process));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    /// 真实网络检测（只读，不启动/不结束任何进程）：cargo test -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_checks() {
        let cfg = crate::config::AppConfig::default();
        let facts = crate::detect::collect(&cfg).await;
        let report = crate::rules::evaluate(&cfg, facts);
        for c in &report.checks {
            println!("{:?}\t{}{}\t{}", c.status, if c.required { "★" } else { " " }, c.title, c.message);
        }
        println!("can_launch = {}", report.can_launch);
        println!("matched processes (list only): {}", crate::process::list(&cfg.process).len());
    }
}
