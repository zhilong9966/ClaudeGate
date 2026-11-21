#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 计划任务以管理员身份调起的防火墙同步模式：同步完立即退出，不打开界面
    if std::env::args().any(|a| a == "--fw-sync") {
        claudegate_lib::firewall_helper();
        return;
    }
    claudegate_lib::run()
}
