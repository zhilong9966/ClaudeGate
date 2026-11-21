//! 网络异常自动终止（运行期守护）
//!
//! PureGate 运行期间（含最小化到托盘）常驻一个监控任务，对每个已启用、且正在运行的受保护应用：
//! - 快速复检（每 interval_sec 秒）：出口 IP 能否获取、是否变化、是否落在受限地区；
//! - 完整复检（每 full_check_minutes 分钟）：按该应用的硬性条件重新检测一遍；
//! - 不是通过 PureGate 启动的应用（monitor_external_launch）：一发现就立即做一次完整检测。
//! 任一项判定为异常，立即结束该应用的全部进程，并发系统通知。
use crate::config::{AppProfile, Guard as GuardCfg};
use crate::state::AppState;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct GuardEvent {
    pub app_id: String,
    /// watching（开始守护）| ok（复检正常）| killed（已终止）| stopped（应用已退出）
    pub kind: String,
    pub message: String,
}

/// 单个应用的守护状态
#[derive(Debug, Clone, Default)]
pub struct AppGuard {
    /// 基准出口 IP：通过 PureGate 启动时取检测报告里的 IP；外部启动时取首次观察到的 IP
    pub baseline: Option<String>,
    /// 出口 IP 连续获取失败次数
    pub fails: u32,
    /// 上次完整复检时间；None = 尚未做过（外部启动的应用会立即做一次）
    pub last_full: Option<Instant>,
    /// 是否是 PureGate 以外启动的
    pub external: bool,
}

impl AppGuard {
    /// 通过 PureGate 检测并启动后调用
    pub fn launched(baseline: String) -> Self {
        Self { baseline: Some(baseline), fails: 0, last_full: Some(Instant::now()), external: false }
    }
}

/// 出口 IP 所在地区是否违反该应用的「支持地区」规则
pub fn region_violation(app: &AppProfile, country: &str) -> Option<String> {
    let rule = app.rule("region")?;
    let cc = country.to_uppercase();
    if cc.is_empty() {
        return None;
    }
    let blocked: Vec<String> = rule.list("blocked_countries").iter().map(|s| s.to_uppercase()).collect();
    let allowed: Vec<String> = rule.list("allowed_countries").iter().map(|s| s.to_uppercase()).collect();
    if blocked.contains(&cc) {
        return Some(format!("出口位于受限地区 {cc}"));
    }
    if !allowed.is_empty() && !allowed.contains(&cc) {
        return Some(format!("出口地区 {cc} 不在允许列表"));
    }
    None
}

/// 快速复检的判定（纯函数，便于测试）。返回 Some(原因) 表示应终止。
/// obs = Ok((出口 IP, 国家码)) 或 Err(错误信息)
pub fn decide_quick(g: &mut AppGuard, cfg: &GuardCfg, app: &AppProfile, obs: Result<(String, String), String>) -> Option<String> {
    match obs {
        Err(e) => {
            g.fails += 1;
            let threshold = cfg.fail_threshold.max(1);
            if cfg.kill_on_fail && g.fails >= threshold {
                Some(format!("出口 IP 连续 {} 次获取失败（{e}）", g.fails))
            } else {
                log::warn!("[{}] 出口 IP 获取失败 {}/{threshold}：{e}", app.id, g.fails);
                None
            }
        }
        Ok((ip, country)) => {
            g.fails = 0;
            if let Some(r) = region_violation(app, &country) {
                return Some(format!("{r}（{ip}）"));
            }
            if crate::allowlist::check(app, &ip) == Some(false) {
                if app.rule(crate::allowlist::CHECK_ID).map(|r| r.required).unwrap_or(false) {
                    return Some(format!("出口 {ip} 不在固定 IP 白名单内"));
                }
                log::warn!("[{}] 出口 {ip} 不在白名单内（非硬性，仅提醒）", app.id);
            }
            match &g.baseline {
                None => {
                    g.baseline = Some(ip);
                    None
                }
                Some(b) if *b != ip && cfg.kill_on_ip_change => Some(format!("出口 IP 变化：{b} → {ip}")),
                _ => None,
            }
        }
    }
}

fn emit(app: &AppHandle, app_id: &str, kind: &str, message: String) {
    let _ = app.emit("guard", GuardEvent { app_id: app_id.into(), kind: kind.into(), message });
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("系统通知发送失败: {e}");
    }
}

/// 防火墙断网保护：按当前运行时状态计算期望规则，与上次下发的不同就同步一次（阻塞等待结果）。
/// 未启用 / 未授权时不做任何事。返回是否已下发成功。
pub async fn fw_sync(handle: &AppHandle) -> bool {
    let st = handle.state::<AppState>();
    let cfg = st.config();
    if !crate::firewall::supported() {
        return false;
    }
    let desired = {
        let fw = st.fw.lock().unwrap();
        let ids: Vec<String> = cfg.apps.iter().filter(|a| a.enabled).map(|a| a.id.clone()).collect();
        let mut d = fw.desired(&ids, cfg.general.firewall.block_until_checked);
        if !cfg.general.firewall.enabled {
            d.values_mut().for_each(|a| a.block = false);
        }
        if d == fw.applied {
            return true;
        }
        d
    };
    let installed = tauri::async_runtime::spawn_blocking(crate::firewall::installed).await.unwrap_or(false);
    if !installed {
        return false;
    }
    let to_apply = desired.clone();
    match tauri::async_runtime::spawn_blocking(move || crate::firewall::apply(&to_apply)).await {
        Ok(Ok(r)) => {
            let blocked: Vec<&String> = desired.iter().filter(|(_, a)| a.block).map(|(id, _)| id).collect();
            if r.ok {
                log::info!("防火墙规则已同步，当前阻断：{blocked:?}（{}）", r.message);
            } else {
                log::warn!("防火墙规则同步部分失败：{}", r.message);
            }
            st.fw.lock().unwrap().applied = desired;
            r.ok
        }
        Ok(Err(e)) => {
            log::error!("防火墙规则同步失败：{e}");
            false
        }
        Err(e) => {
            log::error!("防火墙同步任务异常：{e}");
            false
        }
    }
}

/// 记录应用当前运行的可执行文件路径（防火墙规则需要精确路径）
fn remember_paths(st: &AppState, sys: &sysinfo::System, profile: &AppProfile) {
    let procs = crate::process::list_in(sys, &profile.process);
    let mut fw = st.fw.lock().unwrap();
    let set = fw.known_paths.entry(profile.id.clone()).or_default();
    for p in procs.into_iter().filter(|p| !p.path.is_empty()) {
        set.insert(p.path);
    }
}

/// 启动常驻监控任务（setup 时调用一次）
pub fn spawn_monitor(handle: AppHandle) {
    let _ = crate::netwatch::start();
    tauri::async_runtime::spawn(async move {
        // 启动时先按配置同步一次防火墙（开启「未通过检测前禁止联网」时立即生效）
        fw_sync(&handle).await;
        let mut snap = crate::netwatch::snapshot();
        let mut last_tick = Instant::now();
        loop {
            let g = handle.state::<AppState>().config().general.guard;
            let changed = tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(g.interval_sec.max(5))) => false,
                _ = crate::netwatch::notifier().notified() => true,
            };
            if changed {
                if !g.recheck_on_network_change {
                    continue;
                }
                // 等路由稳定，并把这段时间内的连续变化合并成一次
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let _ = tokio::time::timeout(Duration::from_millis(1), crate::netwatch::notifier().notified()).await;
                let now = crate::netwatch::snapshot();
                let what = crate::netwatch::diff(&snap, &now);
                snap = now;
                let guarding = !handle.state::<AppState>().guards.lock().unwrap().is_empty();
                // 刚复检过（3 秒内）就不重复
                if last_tick.elapsed() < Duration::from_secs(3) {
                    continue;
                }
                if guarding {
                    log::info!("检测到网络变化：{what}，立即复检");
                } else {
                    log::debug!("检测到网络变化：{what}");
                }
            }
            last_tick = Instant::now();
            tick(&handle).await;
        }
    });
}

async fn tick(handle: &AppHandle) {
    let st = handle.state::<AppState>();
    let cfg = st.config();
    let g = cfg.general.guard.clone();
    if !g.enabled {
        st.guards.lock().unwrap().clear();
        return;
    }
    let Ok(client) = crate::http::client(&cfg.network) else { return };
    // IPv6 出口每轮只探测一次，所有应用共用
    let mut v6_cache: Option<crate::detect::ipv6::Ipv6Facts> = None;

    // 每轮只拍一次进程快照，所有应用共用（放到阻塞线程，避免占用异步运行时）
    let Ok(sys) = tauri::async_runtime::spawn_blocking(crate::process::snapshot).await else { return };

    for profile in cfg.apps.iter().filter(|a| a.enabled) {
        let id = profile.id.clone();
        let running = crate::process::is_running_in(&sys, &profile.process);
        // 先在锁内取状态，锁释放后再做异步操作
        let existing: Result<Option<AppGuard>, bool> = {
            let mut guards = st.guards.lock().unwrap();
            if running { Ok(guards.get(&id).cloned()) } else { Err(guards.remove(&id).is_some()) }
        };
        let existing = match existing {
            Ok(e) => e,
            Err(was_guarded) => {
                if was_guarded {
                    log::info!("[{id}] {} 已退出，守护结束", profile.name);
                    emit(handle, &id, "stopped", format!("{} 已退出", profile.name));
                }
                // 应用退出后收回联网授权（开启「未通过检测前禁止联网」时会重新阻断）
                let deauth = st.fw.lock().unwrap().authorized.remove(&id);
                if deauth {
                    fw_sync(handle).await;
                }
                continue;
            }
        };
        remember_paths(&st, &sys, profile);
        let mut ag = match existing {
            Some(a) => a,
            None if g.monitor_external_launch => {
                log::warn!("[{id}] 发现 {} 正在运行但未经 PureGate 检测启动，开始守护并立即完整检测", profile.name);
                emit(handle, &id, "watching", format!("发现 {} 在运行（非 PureGate 启动），正在检测…", profile.name));
                AppGuard { external: true, ..Default::default() }
            }
            None => continue,
        };

        // 快速复检
        let cf = if profile.egress.mode == "google_ecs" {
            crate::detect::trace::fetch(&client, &cfg.network.endpoints.cf_trace).await.ok()
        } else {
            None
        };
        let obs = match crate::detect::egress::fetch(&client, &profile.egress, cf.as_ref()).await {
            Ok(e) => {
                let cc = crate::detect::iprisk::country_of(&client, &cfg, &e.ip).await.unwrap_or_else(|| e.loc.clone());
                Ok((e.ip, cc))
            }
            Err(e) => Err(e.to_string()),
        };
        let mut reason = decide_quick(&mut ag, &g, profile, obs);

        // IPv6 泄露快速复检（VPN 掉线后 IPv6 直连是最隐蔽的泄露）
        if let (None, Some(rule)) = (&reason, profile.rule("ipv6_leak")) {
            if v6_cache.is_none() {
                v6_cache = Some(crate::detect::ipv6::detect(&client, &cfg, profile).await);
            }
            let (ok, msg) = crate::detect::ipv6::judge(v6_cache.as_ref().unwrap(), &rule.list("blocked_countries"), rule.bool("fail_on_any_ipv6", false));
            if !ok && rule.required {
                reason = Some(msg);
            }
        }

        // 完整复检：外部启动的应用首次必做；之后按间隔
        let due = match ag.last_full {
            None => true,
            Some(t) => g.full_check_minutes > 0 && t.elapsed() >= Duration::from_secs(g.full_check_minutes * 60),
        };
        if reason.is_none() && due {
            let facts = crate::detect::collect(&cfg, profile).await;
            let rep = crate::rules::evaluate(profile, facts);
            ag.last_full = Some(Instant::now());
            if rep.can_launch {
                log::info!("[{id}] 完整复检通过");
                let newly = st.fw.lock().unwrap().authorized.insert(id.clone());
                if newly {
                    fw_sync(handle).await;
                }
                if ag.baseline.is_none() {
                    ag.baseline = rep.facts.egress.as_ref().map(|e| e.ip.clone());
                }
            } else {
                let failed: Vec<String> = rep
                    .checks
                    .iter()
                    .filter(|c| c.required && c.status != crate::rules::Status::Pass)
                    .map(|c| format!("{}（{}）", c.title, c.message))
                    .collect();
                reason = Some(format!("完整复检未通过：{}", failed.join("；")));
            }
            st.reports.lock().unwrap().insert(id.clone(), rep);
        }

        match reason {
            Some(r) => {
                let r = if ag.external { format!("{r}（该应用未经 PureGate 检测启动）") } else { r };
                log::warn!("[{id}] 网络异常，自动终止 {}：{r}", profile.name);
                // 先用防火墙切断网络，再结束进程
                {
                    let mut fw = st.fw.lock().unwrap();
                    fw.authorized.remove(&id);
                    if cfg.general.firewall.block_before_kill {
                        fw.killed.insert(id.clone());
                    }
                }
                if cfg.general.firewall.enabled && cfg.general.firewall.block_before_kill && fw_sync(handle).await {
                    log::warn!("[{id}] 已用防火墙切断 {} 的网络", profile.name);
                }
                let k = crate::process::kill_all(&profile.process).await;
                let msg = format!("{r}。已结束 {} 个进程{}", k.killed.len(), if k.remaining.is_empty() { String::new() } else { format!("，残留 {} 个", k.remaining.len()) });
                if g.notify {
                    notify(handle, &format!("PureGate 已终止 {}", profile.name), &msg);
                }
                emit(handle, &id, "killed", msg);
                st.guards.lock().unwrap().remove(&id);
                st.reports.lock().unwrap().remove(&id);
            }
            None => {
                let ip = ag.baseline.clone().unwrap_or_default();
                log::debug!("[{id}] 守护复检正常，出口 {ip}");
                emit(handle, &id, "ok", ip);
                st.guards.lock().unwrap().insert(id, ag);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{claude_profile, Guard};

    fn ok(ip: &str, cc: &str) -> Result<(String, String), String> {
        Ok((ip.into(), cc.into()))
    }

    #[test]
    fn baseline_then_change_kills() {
        let (cfg, app) = (Guard::default(), claude_profile());
        let mut g = AppGuard::default();
        assert_eq!(decide_quick(&mut g, &cfg, &app, ok("1.1.1.1", "US")), None);
        assert_eq!(g.baseline.as_deref(), Some("1.1.1.1"));
        assert_eq!(decide_quick(&mut g, &cfg, &app, ok("1.1.1.1", "US")), None);
        assert!(decide_quick(&mut g, &cfg, &app, ok("2.2.2.2", "US")).unwrap().contains("变化"));
    }

    #[test]
    fn blocked_region_kills_even_without_change() {
        let (cfg, app) = (Guard::default(), claude_profile());
        let mut g = AppGuard::launched("3.3.3.3".into());
        assert!(decide_quick(&mut g, &cfg, &app, ok("3.3.3.3", "CN")).unwrap().contains("受限地区"));
    }

    #[test]
    fn failures_respect_threshold() {
        let (cfg, app) = (Guard::default(), claude_profile()); // 默认连续 2 次
        let mut g = AppGuard::launched("1.1.1.1".into());
        assert_eq!(decide_quick(&mut g, &cfg, &app, Err("timeout".into())), None);
        assert!(decide_quick(&mut g, &cfg, &app, Err("timeout".into())).is_some());
        // 中间成功一次会清零
        let mut g = AppGuard::launched("1.1.1.1".into());
        decide_quick(&mut g, &cfg, &app, Err("timeout".into()));
        decide_quick(&mut g, &cfg, &app, ok("1.1.1.1", "US"));
        assert_eq!(decide_quick(&mut g, &cfg, &app, Err("timeout".into())), None);
    }

    #[test]
    fn switches_off() {
        let cfg = Guard { kill_on_ip_change: false, kill_on_fail: false, ..Guard::default() };
        let app = claude_profile();
        let mut g = AppGuard::launched("1.1.1.1".into());
        assert_eq!(decide_quick(&mut g, &cfg, &app, ok("2.2.2.2", "US")), None);
        assert_eq!(decide_quick(&mut g, &cfg, &app, Err("x".into())), None);
        assert_eq!(decide_quick(&mut g, &cfg, &app, Err("x".into())), None);
    }
}
