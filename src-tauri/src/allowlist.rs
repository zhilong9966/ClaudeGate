//! 固定出口 IP 白名单：账号只在这几个出口上使用，出口不在名单里就禁止启动 / 立即终止。
//! 每行一项，支持单个 IP 或 CIDR 网段（IPv4 / IPv6），# 后面是备注：
//!   66.227.98.150 # 家里
//!   66.227.98.0/24
use crate::config::AppProfile;
use std::net::IpAddr;

pub const CHECK_ID: &str = "egress_allowlist";

/// 解析一项，返回 (网络地址, 前缀长度)
fn parse_entry(entry: &str) -> Option<(IpAddr, u32)> {
    let e = entry.split('#').next()?.trim();
    if e.is_empty() {
        return None;
    }
    let (ip, bits) = match e.split_once('/') {
        Some((ip, b)) => (ip.trim().parse::<IpAddr>().ok()?, b.trim().parse::<u32>().ok()?),
        None => {
            let ip = e.parse::<IpAddr>().ok()?;
            (ip, if ip.is_ipv4() { 32 } else { 128 })
        }
    };
    let max = if ip.is_ipv4() { 32 } else { 128 };
    (bits <= max).then_some((ip, bits))
}

fn contains(net: IpAddr, bits: u32, ip: IpAddr) -> bool {
    match (net, ip) {
        (IpAddr::V4(n), IpAddr::V4(i)) => {
            let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
            (u32::from(n) & mask) == (u32::from(i) & mask)
        }
        (IpAddr::V6(n), IpAddr::V6(i)) => {
            let mask = if bits == 0 { 0 } else { u128::MAX << (128 - bits) };
            (u128::from(n) & mask) == (u128::from(i) & mask)
        }
        _ => false,
    }
}

/// 名单里的有效条目
pub fn entries(app: &AppProfile) -> Vec<String> {
    app.checks.get(CHECK_ID).map(|r| r.list("ips")).unwrap_or_default().into_iter().filter(|e| parse_entry(e).is_some()).collect()
}

pub fn ip_in(list: &[String], ip: &str) -> bool {
    let Ok(ip) = ip.parse::<IpAddr>() else { return false };
    list.iter().filter_map(|e| parse_entry(e)).any(|(n, b)| contains(n, b, ip))
}

/// 白名单已启用且非空时，返回 Some(是否在名单内)；未启用或名单为空返回 None（不做限制）
pub fn check(app: &AppProfile, ip: &str) -> Option<bool> {
    app.rule(CHECK_ID)?;
    let list = entries(app);
    if list.is_empty() {
        return None;
    }
    Some(ip_in(&list, ip))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        let l: Vec<String> = vec!["66.227.98.150 # 家里".into(), "10.0.0.0/8".into(), "2600:1::/32".into(), "garbage".into()];
        assert!(ip_in(&l, "66.227.98.150"));
        assert!(!ip_in(&l, "66.227.98.151"));
        assert!(ip_in(&l, "10.20.30.40"));
        assert!(ip_in(&l, "2600:1:abcd::1"));
        assert!(!ip_in(&l, "2600:2::1"));
        assert!(!ip_in(&l, "not-an-ip"));
    }
}
