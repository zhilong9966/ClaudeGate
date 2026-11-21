//! IPv6 泄露检测
//!
//! 代理 / TUN 只接管了 IPv4 时，本机若有公网 IPv6，应用访问有 AAAA 记录的域名会优先走 IPv6 直连，
//! 绕过代理暴露真实位置。这里检查三件事：
//! 1. 本机网卡上有没有公网 IPv6 地址（仅供参考）；
//! 2. IPv6 出站通不通、出口是谁：直接请求 Cloudflare 的 IPv6 地址 [2606:4700:4700::1111]/cdn-cgi/trace；
//! 3. 应用域名在系统 DNS 里有没有 AAAA 记录；有的话再强制走 IPv6 请求一次该域名，拿到应用侧看到的 IPv6 出口。
use crate::config::{AppConfig, AppProfile};
use serde::Serialize;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Ipv6Facts {
    /// 本机网卡上的公网 IPv6 地址
    pub local_addrs: Vec<String>,
    /// 经 IPv6 访问 Cloudflare 看到的出口（为空表示 IPv6 出站不通）
    pub egress: String,
    pub egress_country: String,
    pub egress_error: String,
    /// 应用域名
    pub app_domain: String,
    /// 应用域名的 AAAA 记录（系统 DNS）
    pub app_aaaa: Vec<String>,
    /// 强制走 IPv6 访问应用域名时，应用侧看到的出口
    pub app_egress: String,
}

/// 公网单播地址 2000::/3（排除本地链路、ULA、文档地址等）
pub fn is_global(ip: &Ipv6Addr) -> bool {
    let s = ip.segments();
    (s[0] & 0xe000) == 0x2000 && s[0] != 0x2001 || (s[0] == 0x2001 && s[1] != 0x0db8 && s[1] != 0x0000)
}

fn local_global_v6() -> Vec<String> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V6(v6) if is_global(&v6) => Some(v6.to_string()),
            _ => None,
        })
        .collect()
}

fn app_domain(app: &AppProfile) -> String {
    let url = if app.egress.mode == "cf_trace" && !app.egress.url.is_empty() {
        app.egress.url.clone()
    } else {
        app.probes.first().map(|p| p.url.clone()).unwrap_or_default()
    };
    url.split("://").nth(1).unwrap_or("").split(['/', ':']).next().unwrap_or("").to_string()
}

/// 解析 trace 文本里的 ip=
fn trace_ip(body: &str) -> String {
    crate::detect::trace::parse(body).ip
}

pub async fn detect(client: &reqwest::Client, cfg: &AppConfig, app: &AppProfile) -> Ipv6Facts {
    let mut f = Ipv6Facts { local_addrs: local_global_v6(), app_domain: app_domain(app), ..Default::default() };
    let timeout = Duration::from_millis(cfg.network.timeout_ms.min(6000));

    // 2. IPv6 出站
    match client.get("https://[2606:4700:4700::1111]/cdn-cgi/trace").timeout(timeout).send().await {
        Ok(r) => match r.text().await {
            Ok(body) => f.egress = trace_ip(&body),
            Err(e) => f.egress_error = e.to_string(),
        },
        Err(e) => f.egress_error = if e.is_connect() || e.is_timeout() { "IPv6 出站不通".into() } else { e.to_string() },
    }
    // 走代理时返回的可能是代理的 IPv4 出口，这不算 IPv6 直连
    if f.egress.parse::<Ipv6Addr>().is_err() {
        f.egress.clear();
    }
    if !f.egress.is_empty() {
        f.egress_country = crate::detect::localdb::country(cfg, &f.egress).unwrap_or_default();
    }

    // 3. 应用域名的 AAAA，及强制 IPv6 访问
    if !f.app_domain.is_empty() {
        if let Ok(Ok(addrs)) = tokio::time::timeout(timeout, tokio::net::lookup_host(format!("{}:443", f.app_domain))).await {
            f.app_aaaa = addrs.filter(|a| a.is_ipv6()).map(|a| a.ip().to_string()).collect();
            f.app_aaaa.dedup();
        }
        if let (Some(first), true) = (f.app_aaaa.first(), !f.egress.is_empty() && app.egress.mode == "cf_trace") {
            if let Ok(ip) = first.parse::<IpAddr>() {
                if let Ok(c6) = crate::http::client_resolving(&cfg.network, &f.app_domain, SocketAddr::new(ip, 443)) {
                    if let Ok(r) = c6.get(format!("https://{}/cdn-cgi/trace", f.app_domain)).timeout(timeout).send().await {
                        f.app_egress = r.text().await.map(|b| trace_ip(&b)).unwrap_or_default();
                    }
                }
            }
        }
    }
    f
}

/// 判定（纯函数）：返回 (是否通过, 说明)
pub fn judge(f: &Ipv6Facts, blocked: &[String], fail_on_any_ipv6: bool) -> (bool, String) {
    let local = if f.local_addrs.is_empty() { "本机无公网 IPv6".to_string() } else { format!("本机有 {} 个公网 IPv6 地址", f.local_addrs.len()) };
    if f.egress.is_empty() {
        return (true, format!("IPv6 出站不通，不会经 IPv6 泄露（{local}）"));
    }
    let aaaa = if f.app_aaaa.is_empty() {
        format!("{} 无 AAAA 记录", f.app_domain)
    } else {
        format!("{} 有 AAAA 记录，应用会优先走 IPv6", f.app_domain)
    };
    let cc = f.egress_country.to_uppercase();
    if blocked.iter().any(|b| b.eq_ignore_ascii_case(&cc)) {
        return (false, format!("IPv6 直连泄露：出口 {}（{cc}）；{aaaa}", f.egress));
    }
    if fail_on_any_ipv6 {
        return (false, format!("存在 IPv6 出口 {}（{cc}），已设置为不允许任何 IPv6；{aaaa}", f.egress));
    }
    (true, format!("IPv6 出口 {}（{}），未落在禁止地区；{aaaa}", f.egress, if cc.is_empty() { "地区未知" } else { &cc }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_ranges() {
        assert!(is_global(&"240e:b8f:1252:3301::1".parse().unwrap()));
        assert!(is_global(&"2606:4700:4700::1111".parse().unwrap()));
        assert!(!is_global(&"fe80::1".parse().unwrap()));
        assert!(!is_global(&"fd00::1".parse().unwrap()));
        assert!(!is_global(&"::1".parse().unwrap()));
        assert!(!is_global(&"2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn judging() {
        let cn = vec!["CN".to_string()];
        let blocked_off = Ipv6Facts { local_addrs: vec!["240e::1".into()], ..Default::default() };
        assert!(judge(&blocked_off, &cn, false).0);

        let leak = Ipv6Facts { egress: "240e::1".into(), egress_country: "CN".into(), app_domain: "claude.ai".into(), ..Default::default() };
        let (ok, msg) = judge(&leak, &cn, false);
        assert!(!ok && msg.contains("泄露"));

        let us = Ipv6Facts { egress: "2600::1".into(), egress_country: "US".into(), ..Default::default() };
        assert!(judge(&us, &cn, false).0);
        assert!(!judge(&us, &cn, true).0);
    }
}
