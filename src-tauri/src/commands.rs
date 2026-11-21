use crate::config::AppConfig;
use crate::detect::localdb::{self, DbStatus};
use crate::process::{KillReport, ProcInfo};
use crate::rules::Report;
use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};

type R<T> = Result<T, String>;

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config()
}

#[tauri::command]
pub fn save_config(state: State<AppState>, cfg: AppConfig) -> R<()> {
    cfg.save(&state.config_path).map_err(|e| {
        log::error!("保存配置失败: {e}");
        e.to_string()
    })?;
    crate::logging::apply(&cfg.general.log);
    log::info!(
        "配置已保存：日志目录 {}，级别 {}，IP 数据源 {}，离线库目录 {}",
        cfg.general.log.dir, cfg.general.log.level, cfg.network.risk_provider, cfg.network.local_db.dir
    );
    *state.cfg.lock().unwrap() = cfg;
    Ok(())
}

#[tauri::command]
pub fn default_config(state: State<AppState>) -> AppConfig {
    // 目录类设置沿用当前解析好的默认路径，避免恢复默认后变成空
    let cur = state.config();
    let mut c = AppConfig::default();
    c.general.log.dir = crate::default_dirs().0.unwrap_or(cur.general.log.dir);
    c.network.local_db.dir = crate::default_dirs().1.unwrap_or(cur.network.local_db.dir);
    c
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
    let t = std::time::Instant::now();
    let facts = crate::detect::collect(&cfg, &app).await;
    let report = crate::rules::evaluate(&app, facts);
    for c in &report.checks {
        log::info!("[{app_id}] {:?}\t{}{}\t{}", c.status, if c.required { "★" } else { "" }, c.title, c.message);
    }
    log::info!("[{app_id}] 检测完成，用时 {} ms，允许启动: {}", t.elapsed().as_millis(), report.can_launch);
    state.reports.lock().unwrap().insert(app_id, report.clone());
    Ok(report)
}

/// 启动应用：必须有一份通过的检测报告；可选启动前快速复检出口 IP
#[tauri::command]
pub async fn launch_app(handle: AppHandle, state: State<'_, AppState>, app_id: String) -> R<String> {
    let cfg = state.config();
    let app = state.app(&app_id)?;
    let fail = |m: String| {
        log::warn!("[{app_id}] 启动被拒绝: {m}");
        m
    };
    let report = state.reports.lock().unwrap().get(&app_id).cloned().ok_or_else(|| fail("请先完成检测".into()))?;
    if !report.can_launch {
        return Err(fail("存在未通过的硬性条件，禁止启动".into()));
    }
    let baseline = report.facts.egress.as_ref().map(|t| t.ip.clone()).unwrap_or_default();
    if app.launcher.recheck_before_launch {
        let c = crate::http::client(&cfg.network).map_err(|e| fail(e.to_string()))?;
        let cf = crate::detect::trace::fetch(&c, &cfg.network.endpoints.cf_trace).await.ok();
        let e = crate::detect::egress::fetch(&c, &app.egress, cf.as_ref()).await.map_err(|e| fail(format!("复检失败：{e}")))?;
        if e.ip != baseline {
            return Err(fail(format!("出口 IP 已变化（{baseline} → {}），请重新检测", e.ip)));
        }
        log::info!("[{app_id}] 启动前复检通过，出口 {baseline}");
    }
    let msg = crate::launcher::launch(&app.launcher).map_err(|e| {
        log::error!("[{app_id}] 启动失败: {e}");
        e.to_string()
    })?;
    log::info!("[{app_id}] {msg}");
    if cfg.general.guard.enabled && !baseline.is_empty() {
        log::info!(
            "[{app_id}] 守护已开启，基准出口 {baseline}，快速复检 {} 秒 / 完整复检 {} 分钟",
            cfg.general.guard.interval_sec, cfg.general.guard.full_check_minutes
        );
        state.guards.lock().unwrap().insert(app_id.clone(), crate::guard::AppGuard::launched(baseline));
        let _ = handle.emit("guard", crate::guard::GuardEvent { app_id, kind: "watching".into(), message: "守护已开启".into() });
    }
    Ok(msg)
}

#[tauri::command]
pub async fn kill_app(handle: AppHandle, state: State<'_, AppState>, app_id: String) -> R<KillReport> {
    let app = state.app(&app_id)?;
    state.guards.lock().unwrap().remove(&app_id);
    log::info!("[{app_id}] 手动关闭");
    let _ = handle.emit("guard", crate::guard::GuardEvent { app_id: app_id.clone(), kind: "stopped".into(), message: String::new() });
    Ok(crate::process::kill_all(&app.process).await)
}

#[tauri::command]
pub fn list_app_processes(state: State<AppState>, app_id: String) -> R<Vec<ProcInfo>> {
    Ok(crate::process::list(&state.app(&app_id)?.process))
}

#[tauri::command]
pub fn geo_db_status(state: State<AppState>) -> DbStatus {
    localdb::status(&state.config().network.local_db)
}

/// 手动更新离线库（force = 全部重新下载）
#[tauri::command]
pub async fn update_geo_db(state: State<'_, AppState>, force: bool) -> R<Vec<String>> {
    let cfg = state.config();
    log::info!("手动更新离线库（强制: {force}）");
    let c = crate::http::client(&cfg.network).map_err(|e| e.to_string())?;
    let mut out = localdb::update(&c, &cfg.network.local_db, force).await;
    if out.is_empty() {
        out.push("所有文件都在更新间隔内，无需更新".into());
    }
    Ok(out)
}

#[tauri::command]
pub fn shortcut_info() -> R<crate::shortcut::ShortcutInfo> {
    crate::shortcut::info().map_err(|e| e.to_string())
}

/// 在桌面创建（或覆盖）指向当前程序的快捷方式
#[tauri::command]
pub fn create_shortcut() -> R<crate::shortcut::ShortcutInfo> {
    crate::shortcut::create().map_err(|e| {
        log::error!("创建桌面快捷方式失败: {e}");
        e.to_string()
    })
}
