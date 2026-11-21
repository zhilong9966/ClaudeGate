//! 查找并彻底结束受保护应用的进程（含子进程树），多轮扫描确认无残留。
use crate::config::{ProcessCfg, ProcessRule};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

#[derive(Debug, Clone, Serialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct KillReport {
    pub killed: Vec<ProcInfo>,
    pub remaining: Vec<ProcInfo>,
}

fn rules(cfg: &ProcessCfg) -> &[ProcessRule] {
    if cfg!(target_os = "macos") { &cfg.macos_rules } else { &cfg.windows_rules }
}

fn matches(r: &ProcessRule, name: &str, path: &str) -> bool {
    let name_ok = r.name.is_empty() || r.name.eq_ignore_ascii_case(name);
    let p = path.to_ascii_lowercase();
    let path_ok = r.path_contains.is_empty() || r.path_contains.iter().any(|s| p.contains(&s.to_ascii_lowercase()));
    name_ok && path_ok && !(r.name.is_empty() && r.path_contains.is_empty())
}

fn snapshot() -> System {
    let mut sys = System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()));
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys
}

fn info(sys: &System, pid: Pid) -> Option<ProcInfo> {
    let p = sys.process(pid)?;
    Some(ProcInfo {
        pid: pid.as_u32(),
        name: p.name().to_string_lossy().into_owned(),
        path: p.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default(),
    })
}

/// 计算需要结束的进程集合
fn targets(sys: &System, cfg: &ProcessCfg) -> Vec<Pid> {
    let me = sysinfo::get_current_pid().ok();
    let rs = rules(cfg);
    let mut set: HashSet<Pid> = HashSet::new();
    let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();

    for (pid, p) in sys.processes() {
        if let Some(pp) = p.parent() {
            children.entry(pp).or_default().push(*pid);
        }
    }

    let has = |path: &str, markers: &[String]| {
        let p = path.to_ascii_lowercase();
        markers.iter().any(|m| !m.is_empty() && p.contains(&m.to_ascii_lowercase()))
    };
    let exe_of = |pid: Pid| sys.process(pid).and_then(|p| p.exe()).map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    // 祖先进程里是否有桌面版主程序（用于"保留独立 CLI"）
    let launched_by_desktop = |mut pid: Pid| -> bool {
        for _ in 0..16 {
            match sys.process(pid).and_then(|p| p.parent()) {
                Some(pp) if has(&exe_of(pp), &cfg.desktop_markers) => return true,
                Some(pp) => pid = pp,
                None => return false,
            }
        }
        false
    };

    for (pid, p) in sys.processes() {
        let name = p.name().to_string_lossy();
        let path = p.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        if !rs.iter().any(|r| matches(r, &name, &path)) {
            continue;
        }
        if cfg.keep_standalone_cli && has(&path, &cfg.cli_markers) && !launched_by_desktop(*pid) {
            continue;
        }
        set.insert(*pid);
    }

    if cfg.kill_children {
        let mut stack: Vec<Pid> = set.iter().copied().collect();
        while let Some(p) = stack.pop() {
            for c in children.get(&p).into_iter().flatten() {
                if set.insert(*c) {
                    stack.push(*c);
                }
            }
        }
    }
    if let Some(me) = me {
        set.remove(&me);
    }

    // 子进程优先（深度大的先杀），避免父进程被杀后子进程被重新托管
    let depth = |mut pid: Pid| {
        let mut d = 0;
        while let Some(pp) = sys.process(pid).and_then(|p| p.parent()) {
            d += 1;
            pid = pp;
            if d > 64 { break; }
        }
        d
    };
    let mut v: Vec<Pid> = set.into_iter().collect();
    v.sort_by_key(|p| std::cmp::Reverse(depth(*p)));
    v
}

/// 列出当前匹配到的 Claude 相关进程
pub fn list(cfg: &ProcessCfg) -> Vec<ProcInfo> {
    let sys = snapshot();
    targets(&sys, cfg).into_iter().filter_map(|p| info(&sys, p)).collect()
}

pub fn is_running(cfg: &ProcessCfg) -> bool {
    !list(cfg).is_empty()
}

pub async fn kill_all(cfg: &ProcessCfg) -> KillReport {
    let mut report = KillReport::default();
    for round in 0..cfg.verify_rounds.max(1) {
        let sys = snapshot();
        let pids = targets(&sys, cfg);
        if pids.is_empty() {
            break;
        }
        // 第一轮先尝试优雅退出，之后强杀
        let graceful = round == 0 && cfg.graceful_timeout_ms > 0 && cfg!(unix);
        for pid in &pids {
            if let (Some(p), Some(i)) = (sys.process(*pid), info(&sys, *pid)) {
                let ok = if graceful { p.kill_with(sysinfo::Signal::Term).unwrap_or(false) } else { p.kill() };
                if ok && !report.killed.iter().any(|k| k.pid == i.pid) {
                    report.killed.push(i);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(if graceful { cfg.graceful_timeout_ms } else { 500 })).await;
    }
    report.remaining = list(cfg);
    report
}
