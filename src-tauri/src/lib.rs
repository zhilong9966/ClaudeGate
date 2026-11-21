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
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

/// 显示并聚焦主窗口（托盘点击、再次双击快捷方式时）
fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

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
        // 只允许运行一个实例：再次打开时唤出已有窗口，避免两个守护同时工作
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
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
            log::info!("===== PureGate 净启 {} 启动（{} / {}） =====", app.package_info().version, std::env::consts::OS, std::env::consts::ARCH);
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
                guards: Mutex::new(HashMap::new()),
            });

            // 托盘：关闭窗口后守护继续在后台运行
            let show = MenuItem::with_id(app, "show", "显示 PureGate", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 PureGate（停止守护）", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("PureGate 净启 · 网络守护中")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            guard::spawn_monitor(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let st = window.state::<state::AppState>();
                if st.config().general.close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                    log::info!("主窗口已隐藏到托盘，守护继续运行");
                }
            }
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
        .expect("error while building PureGate")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let cfg = app.state::<state::AppState>().config();
                log::info!("PureGate 退出（退出时关闭应用: {}）", cfg.general.kill_apps_on_exit);
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
