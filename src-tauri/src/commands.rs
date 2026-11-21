use crate::config::AppConfig;
use crate::process::{KillReport, ProcInfo};
use crate::rules::Report;
use crate::state::AppState;
use tauri::{AppHandle, State};

type R<T> = Result<T, String>;

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config()
}

#[tauri::command]
pub fn save_config(state: State<AppState>, cfg: AppConfig) -> R<()> {
    cfg.save(&state.config_path).map_err(|e| e.to_string())?;
    *state.cfg.lock().unwrap() = cfg;
    Ok(())
}

#[tauri::command]
pub fn default_config() -> AppConfig {
    AppConfig::default()
}

#[tauri::command]
pub fn config_path(state: State<AppState>) -> String {
    state.config_path.to_string_lossy().into_owned()
}

/// 执行全部检测并判定
#[tauri::command]
pub async fn run_checks(state: State<'_, AppState>) -> R<Report> {
    let cfg = state.config();
    let facts = crate::detect::collect(&cfg).await;
    let report = crate::rules::evaluate(&cfg, facts);
    *state.last_report.lock().unwrap() = Some(report.clone());
    Ok(report)
}

/// 启动 Claude：必须有一份通过的检测报告；可选启动前快速复检出口 IP
#[tauri::command]
pub async fn launch_claude(app: AppHandle, state: State<'_, AppState>) -> R<String> {
    let cfg = state.config();
    let report = state.last_report.lock().unwrap().clone().ok_or("请先完成检测")?;
    if !report.can_launch {
        return Err("存在未通过的硬性条件，禁止启动".into());
    }
    let baseline = report.facts.claude.as_ref().map(|t| t.ip.clone()).unwrap_or_default();
    if cfg.launcher.recheck_before_launch {
        let c = crate::http::client(&cfg.network).map_err(|e| e.to_string())?;
        let t = crate::detect::trace::fetch(&c, &cfg.network.endpoints.claude_trace).await.map_err(|e| format!("复检失败：{e}"))?;
        if t.ip != baseline {
            return Err(format!("出口 IP 已变化（{baseline} → {}），请重新检测", t.ip));
        }
    }
    let msg = crate::launcher::launch(&cfg.launcher).map_err(|e| e.to_string())?;
    if cfg.general.guard.enabled && !baseline.is_empty() {
        crate::guard::start(app, baseline);
    }
    Ok(msg)
}

#[tauri::command]
pub async fn kill_claude(app: AppHandle, state: State<'_, AppState>) -> R<KillReport> {
    crate::guard::stop(&app);
    Ok(crate::process::kill_all(&state.config().process).await)
}

#[tauri::command]
pub fn list_claude_processes(state: State<AppState>) -> Vec<ProcInfo> {
    crate::process::list(&state.config().process)
}
