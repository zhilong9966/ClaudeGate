//! 查找并彻底结束受保护应用的进程（含子进程树），多轮扫描确认无残留。
use crate::config::{ProcessCfg, ProcessRule};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

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

/// 名称、路径、命令行三个条件同时满足才命中；为空的条件不限制，但至少要有一个条件
fn matches(r: &ProcessRule, name: &str, path: &str, cmd: &str) -> bool {
    let any = |list: &[String], hay: &str| {
        let h = hay.to_ascii_lowercase();
        list.is_empty() || list.iter().any(|s| h.contains(&s.to_ascii_lowercase()))
    };
    let name_ok = r.name.is_empty() || r.name.eq_ignore_ascii_case(name);
    let has_condition = !r.name.is_empty() || !r.path_contains.is_empty() || !r.cmd_contains.is_empty();
    has_condition && name_ok && any(&r.path_contains, path) && any(&r.cmd_contains, cmd)
}

fn cmdline(p: &sysinfo::Process) -> String {
    p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ")
}

/// 进程快照：只读取匹配所需的路径和命令行（不读内存、环境变量、磁盘 IO、CPU 等），只扫描一遍
pub fn snapshot() -> System {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always).with_cmd(UpdateKind::Always),
    );
    sys
}

/// 旧版快照方式（仅用于性能对比测试）
#[cfg(test)]
fn snapshot_old() -> System {
    use sysinfo::RefreshKind;
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
        let cmd = cmdline(p);
        if !rs.iter().any(|r| matches(r, &name, &path, &cmd)) {
            continue;
        }
        let is_cli = has(&path, &cfg.cli_markers) || has(&cmd, &cfg.cli_markers);
        if cfg.keep_standalone_cli && is_cli && !launched_by_desktop(*pid) {
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

/// 列出当前匹配到的进程
pub fn list(cfg: &ProcessCfg) -> Vec<ProcInfo> {
    list_in(&snapshot(), cfg)
}

/// 在已有快照上列出（守护每轮只拍一次快照，多个应用共用）
pub fn list_in(sys: &System, cfg: &ProcessCfg) -> Vec<ProcInfo> {
    targets(sys, cfg).into_iter().filter_map(|p| info(sys, p)).collect()
}

pub fn is_running_in(sys: &System, cfg: &ProcessCfg) -> bool {
    !targets(sys, cfg).is_empty()
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
    for k in &report.killed {
        log::info!("已结束进程 {} #{} {}", k.name, k.pid, k.path);
    }
    for r in &report.remaining {
        log::warn!("进程残留 {} #{} {}", r.name, r.pid, r.path);
    }
    log::info!("关闭完成：结束 {} 个，残留 {} 个", report.killed.len(), report.remaining.len());
    report
}

#[cfg(test)]
mod tests {
    use super::matches;

    /// 快照耗时对比：cargo test --lib snapshot_cost -- --ignored --nocapture
    #[test]
    #[ignore]
    fn snapshot_cost() {
        let t = std::time::Instant::now();
        let n_old = super::snapshot_old().processes().len();
        let old = t.elapsed();
        let t = std::time::Instant::now();
        let n_new = super::snapshot().processes().len();
        let new = t.elapsed();
        println!("旧快照：{n_old} 个进程 {old:?}；新快照：{n_new} 个进程 {new:?}");
    }
    use crate::config::claude_profile;

    fn hit(name: &str, path: &str, cmd: &str) -> bool {
        let p = claude_profile().process;
        p.windows_rules.iter().any(|r| matches(r, name, path, cmd))
    }

    #[test]
    fn claude_rules_cover_desktop_and_cli() {
        // 桌面版
        assert!(hit("Claude.exe", r"C:\Program Files\WindowsApps\Claude_6.28899.2.0_x64__pzs8sxrjxfjjc\app\Claude.exe", ""));
        // WinGet 安装的 CLI
        assert!(hit("claude.exe", r"C:\Users\A\AppData\Local\Microsoft\WinGet\Packages\Anthropic.ClaudeCode_Microsoft.Winget.Source_8wekyb3d8bbwe\claude.exe", ""));
        // 官方安装脚本的 CLI
        assert!(hit("claude.exe", r"C:\Users\A\.local\bin\claude.exe", ""));
        // npm 安装的 CLI（进程是 node）
        assert!(hit("node.exe", r"D:\Program Files\nodejs\node.exe", r"node C:\Users\A\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js"));
        // 其他 node / 其他程序不能误伤
        assert!(!hit("node.exe", r"D:\Program Files\nodejs\node.exe", r"node C:\proj\server.js"));
        assert!(!hit("Antigravity.exe", r"C:\Users\A\AppData\Local\Programs\antigravity\Antigravity.exe", ""));
        assert!(!hit("claudegate.exe", r"E:\project\ClaudeDeskTool\src-tauri\target\release\claudegate.exe", ""));
    }
}
