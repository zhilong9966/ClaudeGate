//! Cloudflare /cdn-cgi/trace：由 CF 边缘节点直接返回，不经过站点应用服务器、不涉及账号。
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Trace {
    pub ip: String,
    /// 国家码（大写），如 US
    pub loc: String,
    /// CF 机房，如 BOS
    pub colo: String,
    pub warp: String,
    pub host: String,
}

pub fn parse(body: &str) -> Trace {
    let kv: HashMap<&str, &str> = body
        .lines()
        .filter_map(|l| l.split_once('='))
        .collect();
    let g = |k: &str| kv.get(k).map(|s| s.trim().to_string()).unwrap_or_default();
    Trace { ip: g("ip"), loc: g("loc").to_uppercase(), colo: g("colo"), warp: g("warp"), host: g("h") }
}

pub async fn fetch(client: &reqwest::Client, url: &str) -> anyhow::Result<Trace> {
    let body = client.get(url).send().await?.error_for_status()?.text().await?;
    let t = parse(&body);
    if t.ip.is_empty() {
        anyhow::bail!("trace 返回中没有 ip 字段");
    }
    Ok(t)
}
