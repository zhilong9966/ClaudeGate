//! 连通性与延迟：只请求 /cdn-cgi/trace，不访问任何页面或 API，不带 Cookie。
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Avail {
    pub name: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub error: String,
}

pub async fn probe(client: &reqwest::Client, name: &str, url: &str) -> Avail {
    let t = Instant::now();
    let res = client.get(url).send().await;
    let latency_ms = t.elapsed().as_millis() as u64;
    match res {
        Ok(r) if r.status().is_success() => Avail { name: name.into(), ok: true, latency_ms, error: String::new() },
        Ok(r) => Avail { name: name.into(), ok: false, latency_ms, error: format!("HTTP {}", r.status()) },
        Err(e) => Avail { name: name.into(), ok: false, latency_ms, error: e.to_string() },
    }
}
