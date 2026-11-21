//! 网络变化监听：网卡、IP 地址、路由一变化就通知守护立即复检，不必等快速复检间隔。
//! - Windows：系统事件 NotifyIpInterfaceChange / NotifyUnicastIpAddressChange / NotifyRouteChange2
//! - 其他平台：每 2 秒比对一次网卡地址快照
use std::collections::BTreeSet;
use std::sync::OnceLock;
use tokio::sync::Notify;

static CHANGED: OnceLock<Notify> = OnceLock::new();

/// 等待下一次网络变化
pub fn notifier() -> &'static Notify {
    CHANGED.get_or_init(Notify::new)
}

fn signal() {
    notifier().notify_one();
}

/// 当前网卡地址快照（名称 + 地址），用于记录"变了什么"
pub fn snapshot() -> BTreeSet<String> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .map(|i| format!("{} {}", i.name, i.ip()))
        .collect()
}

/// 两次快照的差异，形如 "+Mihomo 198.18.0.1, -以太网 240e::1"
pub fn diff(before: &BTreeSet<String>, after: &BTreeSet<String>) -> String {
    let added = after.difference(before).map(|s| format!("+{s}"));
    let removed = before.difference(after).map(|s| format!("-{s}"));
    let v: Vec<String> = added.chain(removed).collect();
    if v.is_empty() { "路由变化（网卡地址未变）".into() } else { v.join(", ") }
}

/// 返回 true 表示使用系统事件，false 表示退回轮询
#[cfg(windows)]
pub fn start() -> bool {
    use windows_sys::Win32::Foundation::{HANDLE, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        NotifyIpInterfaceChange, NotifyRouteChange2, NotifyUnicastIpAddressChange, MIB_IPFORWARD_ROW2, MIB_IPINTERFACE_ROW,
        MIB_NOTIFICATION_TYPE, MIB_UNICASTIPADDRESS_ROW,
    };
    use windows_sys::Win32::Networking::WinSock::AF_UNSPEC;

    unsafe extern "system" fn on_iface(_: *const core::ffi::c_void, _: *const MIB_IPINTERFACE_ROW, _: MIB_NOTIFICATION_TYPE) {
        signal();
    }
    unsafe extern "system" fn on_addr(_: *const core::ffi::c_void, _: *const MIB_UNICASTIPADDRESS_ROW, _: MIB_NOTIFICATION_TYPE) {
        signal();
    }
    unsafe extern "system" fn on_route(_: *const core::ffi::c_void, _: *const MIB_IPFORWARD_ROW2, _: MIB_NOTIFICATION_TYPE) {
        signal();
    }

    // 句柄在程序整个生命周期内有效，不需要注销
    let mut h1: HANDLE = std::ptr::null_mut();
    let mut h2: HANDLE = std::ptr::null_mut();
    let mut h3: HANDLE = std::ptr::null_mut();
    let r = unsafe {
        [
            NotifyIpInterfaceChange(AF_UNSPEC, Some(on_iface), std::ptr::null(), 0, &mut h1),
            NotifyUnicastIpAddressChange(AF_UNSPEC, Some(on_addr), std::ptr::null(), 0, &mut h2),
            NotifyRouteChange2(AF_UNSPEC, Some(on_route), std::ptr::null(), 0, &mut h3),
        ]
    };
    if r.iter().all(|e| *e == NO_ERROR) {
        log::info!("网络变化监听已启动（系统事件：网卡 / 地址 / 路由）");
        true
    } else {
        log::warn!("网络变化系统事件注册失败 {r:?}，改用轮询");
        start_polling();
        false
    }
}

#[cfg(not(windows))]
pub fn start() -> bool {
    start_polling();
    log::info!("网络变化监听已启动（每 2 秒比对网卡地址）");
    false
}

fn start_polling() {
    std::thread::spawn(|| {
        let mut last = snapshot();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let now = snapshot();
            if now != last {
                last = now;
                signal();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn native_events_register() {
        assert!(start(), "Windows 网络事件注册失败");
    }

    #[test]
    fn diffs() {
        let a: BTreeSet<String> = ["以太网 1.1.1.1".to_string(), "Mihomo 198.18.0.1".to_string()].into();
        let b: BTreeSet<String> = ["以太网 1.1.1.1".to_string()].into();
        assert_eq!(diff(&a, &b), "-Mihomo 198.18.0.1");
        assert_eq!(diff(&a, &a), "路由变化（网卡地址未变）");
    }
}
