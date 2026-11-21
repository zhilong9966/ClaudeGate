//! 独立 HTTP 客户端：无 Cookie 存储、不携带任何 Claude 登录态。
use crate::config::Network;
use std::time::Duration;

pub fn client(net: &Network) -> anyhow::Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .timeout(Duration::from_millis(net.timeout_ms))
        .user_agent(net.user_agent.clone())
        .redirect(reqwest::redirect::Policy::limited(3));
    if !net.proxy.trim().is_empty() {
        b = b.proxy(reqwest::Proxy::all(net.proxy.trim())?);
    }
    Ok(b.build()?)
}

/// 从任意文本中提取第一个 IPv4
pub fn first_ipv4(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}
