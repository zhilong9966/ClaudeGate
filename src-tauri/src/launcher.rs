//! 启动 Claude 桌面版。
use crate::config::Launcher;
use std::process::Command;

#[cfg(windows)]
fn hidden(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}

#[cfg(windows)]
pub fn launch(l: &Launcher) -> anyhow::Result<String> {
    let exe = if !l.windows_exe_path.is_empty() {
        l.windows_exe_path.clone()
    } else {
        // 旧版 Squirrel 安装路径
        std::env::var("LOCALAPPDATA").map(|p| format!("{p}\\AnthropicClaude\\claude.exe")).unwrap_or_default()
    };
    let use_exe = match l.windows_mode.as_str() {
        "exe" => true,
        "aumid" => false,
        _ => l.windows_aumid.is_empty() && std::path::Path::new(&exe).exists(),
    };
    if use_exe {
        hidden(&mut Command::new(&exe)).spawn()?;
        Ok(format!("已启动 {exe}"))
    } else {
        // MSIX 包（WindowsApps）只能通过 AppUserModelID 启动
        let target = format!("shell:AppsFolder\\{}", l.windows_aumid);
        hidden(&mut Command::new("explorer.exe")).arg(&target).spawn()?;
        Ok(format!("已启动 {target}"))
    }
}

#[cfg(target_os = "macos")]
pub fn launch(l: &Launcher) -> anyhow::Result<String> {
    let st = Command::new("open").args(["-b", &l.macos_bundle_id]).status()?;
    if st.success() {
        return Ok(format!("已启动 {}", l.macos_bundle_id));
    }
    let st = Command::new("open").arg("-a").arg(&l.macos_app_path).status()?;
    anyhow::ensure!(st.success(), "无法启动 {}", l.macos_app_path);
    Ok(format!("已启动 {}", l.macos_app_path))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn launch(_l: &Launcher) -> anyhow::Result<String> {
    anyhow::bail!("当前平台不支持")
}
