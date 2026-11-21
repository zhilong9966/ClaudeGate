//! 桌面快捷方式：Windows 生成 .lnk，macOS 在桌面放 .app 的替身（符号链接）。
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct ShortcutInfo {
    pub path: String,
    pub exists: bool,
    /// 快捷方式指向的程序
    pub target: String,
}

#[cfg(windows)]
fn powershell(script: &str) -> anyhow::Result<String> {
    use std::os::windows::process::CommandExt;
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()?;
    anyhow::ensure!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr).trim());
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// PowerShell 单引号字符串转义
#[cfg(windows)]
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(windows)]
fn desktop_dir() -> anyhow::Result<PathBuf> {
    // 用系统 API 取桌面路径，兼容 OneDrive 重定向的桌面
    Ok(PathBuf::from(powershell("[Environment]::GetFolderPath('Desktop')")?))
}

#[cfg(windows)]
fn target() -> anyhow::Result<PathBuf> {
    Ok(std::env::current_exe()?)
}

#[cfg(windows)]
fn link_path() -> anyhow::Result<PathBuf> {
    Ok(desktop_dir()?.join("PureGate 净启.lnk"))
}

#[cfg(windows)]
pub fn create() -> anyhow::Result<ShortcutInfo> {
    let exe = target()?;
    let lnk = link_path()?;
    let dir = exe.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({lnk}); \
         $s.TargetPath = {exe}; $s.WorkingDirectory = {dir}; $s.IconLocation = {icon}; \
         $s.Description = 'PureGate 净启 - AI 桌面应用安全启动器'; $s.Save()",
        lnk = ps_quote(&lnk.to_string_lossy()),
        exe = ps_quote(&exe.to_string_lossy()),
        dir = ps_quote(&dir),
        icon = ps_quote(&format!("{},0", exe.to_string_lossy())),
    );
    powershell(&script)?;
    // 改名前创建的旧快捷方式一并清理
    let _ = std::fs::remove_file(desktop_dir()?.join("ClaudeGate.lnk"));
    log::info!("已创建桌面快捷方式 {} → {}", lnk.display(), exe.display());
    info()
}

#[cfg(windows)]
pub fn info() -> anyhow::Result<ShortcutInfo> {
    let lnk = link_path()?;
    Ok(ShortcutInfo { exists: lnk.exists(), path: lnk.to_string_lossy().into_owned(), target: target()?.to_string_lossy().into_owned() })
}

#[cfg(target_os = "macos")]
fn target() -> anyhow::Result<PathBuf> {
    // .../PureGate.app/Contents/MacOS/claudegate → .../PureGate.app
    let exe = std::env::current_exe()?;
    Ok(exe.ancestors().find(|p| p.extension().map(|e| e == "app").unwrap_or(false)).map(|p| p.to_path_buf()).unwrap_or(exe))
}

#[cfg(target_os = "macos")]
fn link_path() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME")?;
    Ok(PathBuf::from(home).join("Desktop").join("PureGate 净启"))
}

#[cfg(target_os = "macos")]
pub fn create() -> anyhow::Result<ShortcutInfo> {
    let app = target()?;
    let link = link_path()?;
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link)?;
    }
    std::os::unix::fs::symlink(&app, &link)?;
    log::info!("已创建桌面快捷方式 {} → {}", link.display(), app.display());
    info()
}

#[cfg(target_os = "macos")]
pub fn info() -> anyhow::Result<ShortcutInfo> {
    let link = link_path()?;
    Ok(ShortcutInfo {
        exists: link.symlink_metadata().is_ok(),
        path: link.to_string_lossy().into_owned(),
        target: target()?.to_string_lossy().into_owned(),
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn create() -> anyhow::Result<ShortcutInfo> {
    anyhow::bail!("当前平台不支持")
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn info() -> anyhow::Result<ShortcutInfo> {
    anyhow::bail!("当前平台不支持")
}
