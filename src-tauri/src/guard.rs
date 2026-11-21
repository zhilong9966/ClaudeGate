//! 运行期守护：应用运行期间定时检测其出口 IP，
//! IP 变化或获取失败（如 VPN 掉线）时立即结束该应用，避免直连裸奔。
use crate::state::AppState;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct GuardEvent {
    pub app_id: String,
    pub kind: String,
    pub message: String,
}

pub fn start(app: AppHandle, app_id: String, baseline_ip: String) {
    let gen = app.state::<AppState>().bump_guard(&app_id);
    tauri::async_runtime::spawn(async move {
        let emit = |kind: &str, message: String| {
            let _ = app.emit("guard", GuardEvent { app_id: app_id.clone(), kind: kind.into(), message });
        };
        loop {
            let st = app.state::<AppState>();
            let cfg = st.config();
            let g = cfg.general.guard.clone();
            if !g.enabled || st.guard_gen(&app_id) != gen {
                return;
            }
            tokio::time::sleep(Duration::from_secs(g.interval_sec.max(5))).await;
            let Ok(profile) = st.app(&app_id) else { return };
            if st.guard_gen(&app_id) != gen {
                return;
            }
            if !crate::process::is_running(&profile.process) {
                log::info!("[{app_id}] 应用已退出，守护结束");
                emit("stopped", format!("{} 已退出，守护结束", profile.name));
                return;
            }
            let reason = match crate::http::client(&cfg.network) {
                Ok(c) => {
                    let cf = crate::detect::trace::fetch(&c, &cfg.network.endpoints.cf_trace).await.ok();
                    match crate::detect::egress::fetch(&c, &profile.egress, cf.as_ref()).await {
                        Ok(e) if e.ip != baseline_ip && g.kill_on_ip_change => {
                            Some(format!("{} 出口 IP 变化：{baseline_ip} → {}", profile.name, e.ip))
                        }
                        Ok(_) => None,
                        Err(e) if g.kill_on_fail => Some(format!("{} 出口 IP 获取失败：{e}", profile.name)),
                        Err(_) => None,
                    }
                }
                Err(e) => Some(e.to_string()),
            };
            if let Some(msg) = reason {
                log::warn!("[{app_id}] 守护触发：{msg}，开始结束进程");
                let r = crate::process::kill_all(&profile.process).await;
                emit("killed", format!("{msg}，已结束 {} 个进程", r.killed.len()));
                return;
            }
            log::debug!("[{app_id}] 守护复检正常，出口 {baseline_ip}");
            emit("ok", baseline_ip.clone());
        }
    });
}

pub fn stop(app: &AppHandle, app_id: &str) {
    app.state::<AppState>().bump_guard(app_id);
}
