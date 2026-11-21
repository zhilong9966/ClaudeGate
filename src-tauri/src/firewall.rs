//! 防火墙断网保护（Windows）
//!
//! 修改 Windows 防火墙需要管理员权限。为避免每次断网都弹 UAC，采用"一次授权"：
//! 1. 用户在设置里点「授权」→ 弹一次 UAC → 注册计划任务 \PureGate\PureGate Firewall Sync
//!    （以当前用户、最高权限运行 `claudegate.exe --fw-sync`）；
//! 2. 之后 PureGate 只需把期望状态写进 state.json 并 `schtasks /run` 这个任务，
//!    提权后的同步程序按状态增删防火墙规则，结果写回 result.json。
//! 安全限制：同步程序只操作名为 PureGate-Block-<应用id> 的规则，且只创建「出站阻止」规则，
//! 路径必须是存在的绝对 .exe 路径。
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const TASK_PATH: &str = "\\PureGate\\";
pub const TASK_NAME: &str = "PureGate Firewall Sync";
pub const RULE_PREFIX: &str = "PureGate-Block-";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FwApp {
    pub block: bool,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FwState {
    pub seq: u64,
    pub apps: BTreeMap<String, FwApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FwResult {
    pub seq: u64,
    pub ok: bool,
    pub message: String,
}

/// 运行时状态（放在 AppState 里）
#[derive(Debug, Default)]
pub struct FwRuntime {
    /// 因网络异常被终止、尚未重新通过检测的应用
    pub killed: BTreeSet<String>,
    /// 已通过检测、允许联网的应用（通过 PureGate 启动，或守护完整复检通过）
    pub authorized: BTreeSet<String>,
    /// 见过的各应用可执行文件路径（防火墙规则要精确路径）
    pub known_paths: BTreeMap<String, BTreeSet<String>>,
    /// 上次成功下发的状态
    pub applied: BTreeMap<String, FwApp>,
}

impl FwRuntime {
    /// 计算期望状态
    pub fn desired(&self, app_ids: &[String], block_until_checked: bool) -> BTreeMap<String, FwApp> {
        app_ids
            .iter()
            .map(|id| {
                let block = self.killed.contains(id) || (block_until_checked && !self.authorized.contains(id));
                let paths = self.known_paths.get(id).map(|s| s.iter().cloned().collect()).unwrap_or_default();
                (id.clone(), FwApp { block, paths })
            })
            .collect()
    }
}

fn dir() -> PathBuf {
    // 提权后的同步程序与 PureGate 是同一用户，LOCALAPPDATA 相同
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("com.zhilong.claudegate").join("firewall")
}

fn state_path() -> PathBuf {
    dir().join("state.json")
}

fn result_path() -> PathBuf {
    dir().join("result.json")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 规则只允许指向存在的绝对 .exe 路径
fn valid_path(p: &str) -> bool {
    let path = std::path::Path::new(p);
    path.is_absolute() && !p.contains('"') && p.to_ascii_lowercase().ends_with(".exe") && path.exists()
}

// ---------------------------------------------------------------- Windows 实现

#[cfg(windows)]
fn run(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(cmd).args(args).creation_flags(0x0800_0000).output()
}

#[cfg(windows)]
pub fn supported() -> bool {
    true
}

#[cfg(windows)]
pub fn installed() -> bool {
    run("schtasks", &["/query", "/tn", &format!("{TASK_PATH}{TASK_NAME}")]).map(|o| o.status.success()).unwrap_or(false)
}

/// 弹一次 UAC，注册提权计划任务
#[cfg(windows)]
pub fn install() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let user = format!("{}\\{}", std::env::var("USERDOMAIN").unwrap_or_default(), std::env::var("USERNAME")?);
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let script = format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         $a = New-ScheduledTaskAction -Execute {exe} -Argument '--fw-sync'\r\n\
         $p = New-ScheduledTaskPrincipal -UserId {user} -LogonType Interactive -RunLevel Highest\r\n\
         $s = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable -MultipleInstances Queue -ExecutionTimeLimit (New-TimeSpan -Minutes 2)\r\n\
         Register-ScheduledTask -TaskPath {tp} -TaskName {tn} -Action $a -Principal $p -Settings $s -Force | Out-Null\r\n",
        exe = q(&exe),
        user = q(&user),
        tp = q(TASK_PATH),
        tn = q(TASK_NAME),
    );
    elevate(&script, "install.ps1")?;
    anyhow::ensure!(installed(), "授权未完成（UAC 被取消或注册计划任务失败）");
    log::info!("防火墙断网保护已授权：计划任务 {TASK_PATH}{TASK_NAME} → {exe}");
    Ok(())
}

/// 弹一次 UAC，删除计划任务和所有 PureGate 防火墙规则
#[cfg(windows)]
pub fn uninstall() -> anyhow::Result<()> {
    let script = format!(
        "Get-NetFirewallRule -DisplayName '{RULE_PREFIX}*' -ErrorAction SilentlyContinue | Remove-NetFirewallRule\r\n\
         Unregister-ScheduledTask -TaskPath '{TASK_PATH}' -TaskName '{TASK_NAME}' -Confirm:$false -ErrorAction SilentlyContinue\r\n"
    );
    elevate(&script, "uninstall.ps1")?;
    anyhow::ensure!(!installed(), "取消授权未完成（UAC 被取消）");
    log::info!("防火墙断网保护已取消授权，PureGate 防火墙规则已清除");
    Ok(())
}

/// 把脚本写到文件，以管理员身份运行并等待结束（弹 UAC）
#[cfg(windows)]
fn elevate(script: &str, name: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir())?;
    let path = dir().join(name);
    // 带 BOM 的 UTF-8，PowerShell 5 才能正确读中文路径
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(script.as_bytes());
    std::fs::write(&path, bytes)?;
    let outer = format!(
        "Start-Process -FilePath powershell.exe -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File \"{}\"'",
        path.to_string_lossy().replace('\'', "''")
    );
    let out = run("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", &outer])?;
    let _ = std::fs::remove_file(&path);
    anyhow::ensure!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr).trim());
    Ok(())
}

/// 下发期望状态：写 state.json → 运行计划任务 → 等待 result.json（最多 6 秒）
#[cfg(windows)]
pub fn apply(apps: &BTreeMap<String, FwApp>) -> anyhow::Result<FwResult> {
    std::fs::create_dir_all(dir())?;
    let seq = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos() as u64;
    let state = FwState { seq, apps: apps.clone() };
    std::fs::write(state_path(), serde_json::to_vec_pretty(&state)?)?;
    let out = run("schtasks", &["/run", "/tn", &format!("{TASK_PATH}{TASK_NAME}")])?;
    anyhow::ensure!(out.status.success(), "运行防火墙同步任务失败：{}", String::from_utf8_lossy(&out.stderr).trim());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while std::time::Instant::now() < deadline {
        if let Ok(r) = std::fs::read(result_path()).map_err(anyhow::Error::from).and_then(|b| Ok(serde_json::from_slice::<FwResult>(&b)?)) {
            if r.seq == seq {
                return Ok(r);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    anyhow::bail!("等待防火墙同步结果超时")
}

/// 提权后的同步程序入口：`claudegate.exe --fw-sync`
#[cfg(windows)]
pub fn helper_main() {
    let state: FwState = std::fs::read(state_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut notes = vec![];
    let mut ok = true;
    for (id, app) in &state.apps {
        if !valid_id(id) {
            ok = false;
            notes.push(format!("忽略非法应用 id {id}"));
            continue;
        }
        let name = format!("name={RULE_PREFIX}{id}");
        // 先删后加，保证幂等（规则不存在时 netsh 会报错，忽略）
        let _ = run("netsh", &["advfirewall", "firewall", "delete", "rule", &name]);
        if !app.block {
            continue;
        }
        let mut added = 0;
        for p in app.paths.iter().filter(|p| valid_path(p)) {
            let program = format!("program={p}");
            match run("netsh", &["advfirewall", "firewall", "add", "rule", &name, "dir=out", "action=block", &program, "enable=yes", "profile=any"]) {
                Ok(o) if o.status.success() => added += 1,
                Ok(o) => {
                    ok = false;
                    notes.push(format!("{id} {p}: {}", String::from_utf8_lossy(&o.stdout).trim()));
                }
                Err(e) => {
                    ok = false;
                    notes.push(format!("{id} {p}: {e}"));
                }
            }
        }
        notes.push(format!("{id}: 已阻断 {added} 个程序"));
    }
    let result = FwResult { seq: state.seq, ok, message: notes.join("；") };
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(result_path(), serde_json::to_vec(&result).unwrap_or_default());
}

// ---------------------------------------------------------------- 其他平台（暂不支持，见 docs/TODO.md）

#[cfg(not(windows))]
pub fn supported() -> bool {
    false
}
#[cfg(not(windows))]
pub fn installed() -> bool {
    false
}
#[cfg(not(windows))]
pub fn install() -> anyhow::Result<()> {
    anyhow::bail!("防火墙断网保护目前只支持 Windows")
}
#[cfg(not(windows))]
pub fn uninstall() -> anyhow::Result<()> {
    anyhow::bail!("防火墙断网保护目前只支持 Windows")
}
#[cfg(not(windows))]
pub fn apply(_: &BTreeMap<String, FwApp>) -> anyhow::Result<FwResult> {
    anyhow::bail!("防火墙断网保护目前只支持 Windows")
}
#[cfg(not(windows))]
pub fn helper_main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_paths() {
        assert!(valid_id("claude") && valid_id("anti-gravity_2"));
        assert!(!valid_id("") && !valid_id("a b") && !valid_id("x\" y"));
        assert!(!valid_path("relative.exe"));
        assert!(!valid_path("C:\\does\\not\\exist.exe"));
        let me = std::env::current_exe().unwrap();
        if me.extension().map(|e| e == "exe").unwrap_or(false) {
            assert!(valid_path(&me.to_string_lossy()));
        }
    }

    #[test]
    fn desired_state() {
        let ids = vec!["claude".to_string(), "codex".to_string()];
        let mut rt = FwRuntime::default();
        rt.known_paths.entry("claude".into()).or_default().insert("C:\\a\\Claude.exe".into());
        // 默认：都不阻断
        let d = rt.desired(&ids, false);
        assert!(!d["claude"].block && !d["codex"].block);
        assert_eq!(d["claude"].paths, vec!["C:\\a\\Claude.exe".to_string()]);
        // 被终止 → 阻断
        rt.killed.insert("claude".into());
        assert!(rt.desired(&ids, false)["claude"].block);
        // 未通过检测前禁止联网：未授权的都阻断，授权的放行
        rt.killed.clear();
        rt.authorized.insert("codex".into());
        let d = rt.desired(&ids, true);
        assert!(d["claude"].block && !d["codex"].block);
    }
}
