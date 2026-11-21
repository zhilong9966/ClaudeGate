//! 用户自定义站点：出口 IP（经该站点看到的本机 IP）+ 入口 IP（该域名被解析到哪里）。
use crate::config::{AppConfig, CustomSite};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Default)]
pub struct SiteFacts {
    pub name: String,
    pub domain: String,
    pub egress_ip: String,
    pub egress_country: String,
    pub egress_error: String,
    pub ingress_ips: Vec<String>,
    pub ingress_country: String,
    pub ingress_error: String,
}

pub async fn collect(client: &reqwest::Client, cfg: &AppConfig) -> Vec<SiteFacts> {
    futures::future::join_all(cfg.custom_sites.iter().map(|s| one(client, cfg, s))).await
}

async fn one(client: &reqwest::Client, cfg: &AppConfig, s: &CustomSite) -> SiteFacts {
    let mut f = SiteFacts { name: s.name.clone(), domain: s.domain.clone(), ..Default::default() };

    let egress: anyhow::Result<String> = match s.egress_mode.as_str() {
        "cf_trace" => crate::detect::trace::fetch(client, &format!("https://{}/cdn-cgi/trace", s.domain)).await.map(|t| t.ip),
        "ip_echo" => echo(client, &s.egress_url, &s.egress_json_field).await,
        _ => Ok(String::new()),
    };
    match egress {
        Ok(ip) if !ip.is_empty() => {
            f.egress_country = crate::detect::iprisk::country_of(client, cfg, &ip).await.unwrap_or_default();
            f.egress_ip = ip;
        }
        Ok(_) => {}
        Err(e) => f.egress_error = e.to_string(),
    }

    if s.resolve_ingress {
        match tokio::net::lookup_host(format!("{}:443", s.domain)).await {
            Ok(addrs) => {
                f.ingress_ips = addrs.filter(|a| a.is_ipv4()).map(|a| a.ip().to_string()).collect();
                f.ingress_ips.dedup();
                if let Some(ip) = f.ingress_ips.first() {
                    f.ingress_country = crate::detect::iprisk::country_of(client, cfg, ip).await.unwrap_or_default();
                }
            }
            Err(e) => f.ingress_error = e.to_string(),
        }
    }
    f
}

async fn echo(client: &reqwest::Client, url: &str, field: &str) -> anyhow::Result<String> {
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
