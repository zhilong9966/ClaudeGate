//! 应用出口 IP：该厂商服务端看到的本机 IP。
use crate::config::Egress as EgressCfg;
use crate::detect::trace::Trace;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Egress {
    pub ip: String,
    /// 国家码（大写），能取到时填
    pub loc: String,
    pub colo: String,
    /// 厂商侧看到的网段（google_ecs 模式）
    pub vendor_subnet: String,
    pub mode: String,
}

/// cf：全局 Cloudflare 出口（google_ecs 模式需要它提供完整 IP）
pub async fn fetch(client: &reqwest::Client, e: &EgressCfg, cf: Option<&Trace>) -> anyhow::Result<Egress> {
    match e.mode.as_str() {
        "ip_echo" => {
            let ip = echo(client, &e.url, &e.json_field).await?;
            Ok(Egress { ip, mode: e.mode.clone(), ..Default::default() })
        }
        "google_ecs" => {
            let v: Value = client.get(&e.url).send().await?.error_for_status()?.json().await?;
            let subnet = v
                .get("Answer")
                .and_then(|a| a.as_array())
                .into_iter()
                .flatten()
                .filter_map(|x| x.get("data").and_then(|d| d.as_str()))
                .find_map(|d| d.strip_prefix("edns0-client-subnet ").map(|s| s.trim().to_string()))
                .ok_or_else(|| anyhow::anyhow!("Google 未返回客户端网段"))?;
            let cf = cf.ok_or_else(|| anyhow::anyhow!("CF 出口获取失败，无法确定完整 IP"))?;
            anyhow::ensure!(
                in_subnet(&cf.ip, &subnet),
                "Google 看到的出口网段 {subnet} 与通用出口 {} 不一致（分流规则不同）",
                cf.ip
            );
            Ok(Egress { ip: cf.ip.clone(), loc: cf.loc.clone(), colo: cf.colo.clone(), vendor_subnet: subnet, mode: e.mode.clone() })
        }
        _ => {
            let t = crate::detect::trace::fetch(client, &e.url).await?;
            Ok(Egress { ip: t.ip, loc: t.loc, colo: t.colo, vendor_subnet: String::new(), mode: "cf_trace".into() })
        }
    }
}

/// IPv4 是否落在 a.b.c.d/n 网段内
fn in_subnet(ip: &str, cidr: &str) -> bool {
    let (net, bits) = cidr.split_once('/').unwrap_or((cidr, "32"));
    let (Ok(ip), Ok(net), Ok(bits)) = (ip.parse::<std::net::Ipv4Addr>(), net.parse::<std::net::Ipv4Addr>(), bits.parse::<u32>()) else {
        return false;
    };
    let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits.min(32)) };
    (u32::from(ip) & mask) == (u32::from(net) & mask)
}

pub async fn echo(client: &reqwest::Client, url: &str, field: &str) -> anyhow::Result<String> {
    let text = client.get(url).send().await?.error_for_status()?.text().await?;
    if !field.is_empty() {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(ip) = v.get(field).and_then(|x| x.as_str()) {
                return Ok(ip.to_string());
            }
        }
    }
    crate::http::first_ipv4(&text).ok_or_else(|| anyhow::anyhow!("返回内容中没有 IP"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn subnet() {
        assert!(super::in_subnet("66.227.98.150", "66.227.98.0/24"));
        assert!(!super::in_subnet("66.227.99.150", "66.227.98.0/24"));
    }
}
