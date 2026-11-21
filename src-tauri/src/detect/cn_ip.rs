//! 国内出口 IP（直连线路看到的 IP），仅用于展示和对比。
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct CnIp {
    pub ip: String,
    /// 来源站点返回的原始归属地文字（如有）
    pub raw: String,
    pub source: String,
}

pub async fn fetch(client: &reqwest::Client, urls: &[String]) -> anyhow::Result<CnIp> {
    for url in urls {
        let Ok(resp) = client.get(url).send().await else { continue };
        let Ok(text) = resp.text().await else { continue };
        if let Some(ip) = crate::http::first_ipv4(&text) {
            // my.ip.cn 形如 "ip：x.x.x.x 归属地：中国 上海 上海  电信"
            let raw = text
                .split("归属地：")
                .nth(1)
                .map(|s| s.lines().next().unwrap_or("").trim().to_string())
                .unwrap_or_default();
            return Ok(CnIp { ip, raw, source: url.clone() });
        }
    }
    anyhow::bail!("所有国内 IP 源均失败")
}
