//! 运行期守护：Claude 运行期间定时检测 Claude 出口 IP，
//! IP 变化或获取失败（如 VPN 掉线）时立即结束 Claude，避免直连裸奔。
use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct GuardEvent {
    pub kind: String,
    pub message: String,
}

pub fn start(app: AppHandle, baseline_ip: String) {
    let state = app.state::<AppState>();
    let gen = state.guard_gen.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        loop {
            let cfg = app.state::<AppState>().config();
            let g = cfg.general.guard.clone();
            if !g.enabled || app.state::<AppState>().guard_gen.load(Ordering::SeqCst) != gen {
                return;
            }
            tokio::time::sleep(Duration::from_secs(g.interval_sec.max(5))).await;
            if app.state::<AppState>().guard_gen.load(Ordering::SeqCst) != gen {
                return;
            }
            if !crate::process::is_running(&cfg.process) {
                let _ = app.emit("guard", GuardEvent { kind: "stopped".into(), message: "Claude 已退出，守护结束".into() });
                return;
            }
            let reason = match crate::http::client(&cfg.network) {
                Ok(c) => match crate::detect::trace::fetch(&c, &cfg.network.endpoints.claude_trace).await {
                    Ok(t) if t.ip != baseline_ip && g.kill_on_ip_change => Some(format!("Claude 出口 IP 变化：{baseline_ip} → {}", t.ip)),
                    Ok(_) => None,
                    Err(e) if g.kill_on_fail => Some(format!("Claude 出口 IP 获取失败：{e}")),
                    Err(_) => None,
                },
                Err(e) => Some(e.to_string()),
            };
            if let Some(msg) = reason {
                let r = crate::process::kill_all(&cfg.process).await;
                let _ = app.emit("guard", GuardEvent { kind: "killed".into(), message: format!("{msg}，已结束 {} 个进程", r.killed.len()) });
                return;
            }
            let _ = app.emit("guard", GuardEvent { kind: "ok".into(), message: baseline_ip.clone() });
        }
    });
}

pub fn stop(app: &AppHandle) {
    app.state::<AppState>().guard_gen.fetch_add(1, Ordering::SeqCst);
}
