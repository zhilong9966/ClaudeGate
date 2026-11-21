//! DNS 泄露检测：访问随机子域名触发解析，再向权威 DNS 记录方查询"是哪些解析器来问的"。
//! 解析器在国内 → 说明 DNS 走了直连（泄露）。
use crate::config::AppConfig;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Default)]
pub struct DnsServer {
    pub ip: String,
    pub country_code: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DnsLeak {
    pub servers: Vec<DnsServer>,
}

pub async fn detect(client: &reqwest::Client, cfg: &AppConfig) -> anyhow::Result<DnsLeak> {
    let ep = &cfg.network.endpoints;
    let token: String = {
        use rand::Rng;
        let mut r = rand::thread_rng();
        (0..20).map(|_| char::from(b"abcdefghijklmnopqrstuvwxyz0123456789"[r.gen_range(0..36)])).collect()
    };
    // system = 走操作系统 DNS 客户端（与 Claude 桌面版/Chromium 一致，能发现 Windows 多网卡并发查询导致的泄露）
    // http   = 只发 HTTPS 请求（与网页检测站一致）
    // both   = 两者都做（默认，最严格）
    let method = cfg.checks.get("dns_leak").map(|r| r.str("method", "both")).unwrap_or_else(|| "both".into());
    for i in 1..=2 {
        let host = format!("{token}-{i}.{}", ep.dns_leak_base);
        if method != "http" {
            let _ = tokio::time::timeout(Duration::from_secs(2), tokio::net::lookup_host(format!("{host}:443"))).await;
        }
        if method != "system" {
            let _ = client.get(format!("https://{host}/pixel.gif")).timeout(Duration::from_secs(2)).send().await;
        }
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let url = ep.dns_leak_result.replace("{token}", &token);
    let mut ips: Vec<String> = vec![];
    for _ in 0..2 {
        if let Ok(r) = client.get(&url).send().await {
            if let Ok(v) = r.json::<Value>().await {
                ips = v
                    .get("dns_servers")
                    .and_then(|x| x.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
            }
        }
        if !ips.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    if ips.is_empty() {
        anyhow::bail!("未取得 DNS 解析器记录");
    }
    let countries = futures::future::join_all(ips.iter().map(|ip| crate::detect::iprisk::country_of(client, cfg, ip))).await;
    Ok(DnsLeak {
        servers: ips
            .into_iter()
            .zip(countries)
            .map(|(ip, cc)| DnsServer { ip, country_code: cc.unwrap_or_default() })
            .collect(),
    })
}
