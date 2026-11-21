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

/// 对某个应用执行全部检测并判定
#[tauri::command]
pub async fn run_checks(state: State<'_, AppState>, app_id: String) -> R<Report> {
    let cfg = state.config();
    let app = state.app(&app_id)?;
    let facts = crate::detect::collect(&cfg, &app).await;
    let report = crate::rules::evaluate(&app, facts);
    state.reports.lock().unwrap().insert(app_id, report.clone());
    Ok(report)
}

/// 启动应用：必须有一份通过的检测报告；可选启动前快速复检出口 IP
#[tauri::command]
pub async fn launch_app(handle: AppHandle, state: State<'_, AppState>, app_id: String) -> R<String> {
    let cfg = state.config();
    let app = state.app(&app_id)?;
    let report = state.reports.lock().unwrap().get(&app_id).cloned().ok_or("请先完成检测")?;
    if !report.can_launch {
        return Err("存在未通过的硬性条件，禁止启动".into());
    }
    let baseline = report.facts.egress.as_ref().map(|t| t.ip.clone()).unwrap_or_default();
    if app.launcher.recheck_before_launch {
        let c = crate::http::client(&cfg.network).map_err(|e| e.to_string())?;
        let cf = crate::detect::trace::fetch(&c, &cfg.network.endpoints.cf_trace).await.ok();
        let e = crate::detect::egress::fetch(&c, &app.egress, cf.as_ref()).await.map_err(|e| format!("复检失败：{e}"))?;
        if e.ip != baseline {
            return Err(format!("出口 IP 已变化（{baseline} → {}），请重新检测", e.ip));
        }
    }
    let msg = crate::launcher::launch(&app.launcher).map_err(|e| e.to_string())?;
    if cfg.general.guard.enabled && !baseline.is_empty() {
        crate::guard::start(handle, app_id, baseline);
    }
    Ok(msg)
}

#[tauri::command]
pub async fn kill_app(handle: AppHandle, state: State<'_, AppState>, app_id: String) -> R<KillReport> {
    let app = state.app(&app_id)?;
    crate::guard::stop(&handle, &app_id);
    Ok(crate::process::kill_all(&app.process).await)
}

#[tauri::command]
pub fn list_app_processes(state: State<AppState>, app_id: String) -> R<Vec<ProcInfo>> {
    Ok(crate::process::list(&state.app(&app_id)?.process))
}
