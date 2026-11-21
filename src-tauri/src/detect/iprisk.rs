//! IP 属性 / 信任分 / VPN·Proxy·Tor·滥用 —— 可插拔数据源。
use crate::config::AppConfig;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Risk {
    pub provider: String,
    pub ip: String,
    pub country: String,
    pub country_code: String,
    pub city: String,
    pub asn: Option<u64>,
    pub as_org: String,
    pub company_type: String,
    pub is_residential: Option<bool>,
    pub is_datacenter: Option<bool>,
    pub is_vpn: Option<bool>,
    pub is_proxy: Option<bool>,
    pub is_tor: Option<bool>,
    pub is_crawler: Option<bool>,
    pub is_abuser: Option<bool>,
    pub abuser_score: String,
    pub timezone: String,
    pub trust_score: Option<u64>,
}

fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(|x| x.as_bool())
}
fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

pub async fn fetch(client: &reqwest::Client, cfg: &AppConfig, ip: &str) -> anyhow::Result<Risk> {
    let ep = &cfg.network.endpoints;
    match cfg.network.risk_provider.as_str() {
        "ipapi_is" => ipapi_is(client, &ep.iprisk_ipapi_is.replace("{ip}", ip), ip).await,
        "ipnetcoffee" => ipnetcoffee(client, &ep.iprisk_ipnetcoffee.replace("{ip}", ip), ip).await,
        _ => crate::detect::localdb::lookup(cfg, ip),
    }
}

async fn ipnetcoffee(client: &reqwest::Client, url: &str, ip: &str) -> anyhow::Result<Risk> {
    let v: Value = client.get(url).send().await?.error_for_status()?.json().await?;
    Ok(Risk {
        provider: "ip.net.coffee".into(),
        ip: ip.into(),
        country: s(&v, "country"),
        country_code: s(&v, "countryCode").to_uppercase(),
        city: s(&v, "city"),
        asn: v.get("asn").and_then(|x| x.as_u64()),
        as_org: s(&v, "asOrganization"),
        company_type: s(&v, "company_type"),
        is_residential: b(&v, "isResidential"),
        is_datacenter: b(&v, "is_datacenter"),
        is_vpn: b(&v, "is_vpn"),
        is_proxy: b(&v, "is_proxy"),
        is_tor: b(&v, "is_tor"),
        is_crawler: b(&v, "is_crawler"),
        is_abuser: b(&v, "is_abuser"),
        abuser_score: s(&v, "abuser_score"),
        timezone: s(&v, "timezone"),
        trust_score: v.get("trust_score").and_then(|x| x.as_u64()),
    })
}

/// ipapi.is 免费接口：无信任分，按风险标志估算一个分数
async fn ipapi_is(client: &reqwest::Client, url: &str, ip: &str) -> anyhow::Result<Risk> {
    let v: Value = client.get(url).send().await?.error_for_status()?.json().await?;
    let loc = v.get("location").cloned().unwrap_or(Value::Null);
    let asn = v.get("asn").cloned().unwrap_or(Value::Null);
    let company = v.get("company").cloned().unwrap_or(Value::Null);
    let mut r = Risk {
        provider: "ipapi.is".into(),
        ip: ip.into(),
        country: s(&loc, "country"),
        country_code: s(&loc, "country_code").to_uppercase(),
        city: s(&loc, "city"),
        asn: asn.get("asn").and_then(|x| x.as_u64()),
        as_org: s(&asn, "org"),
        company_type: s(&company, "type"),
        is_datacenter: b(&v, "is_datacenter"),
        is_vpn: b(&v, "is_vpn"),
        is_proxy: b(&v, "is_proxy"),
        is_tor: b(&v, "is_tor"),
        is_crawler: b(&v, "is_crawler"),
        is_abuser: b(&v, "is_abuser"),
        timezone: s(&loc, "timezone"),
        ..Default::default()
    };
    r.is_residential = Some(r.company_type == "isp" && r.is_datacenter != Some(true));
    let mut score: i64 = 100;
    for (flag, penalty) in [(r.is_datacenter, 30), (r.is_vpn, 30), (r.is_proxy, 30), (r.is_tor, 60), (r.is_abuser, 40), (r.is_crawler, 20)] {
        if flag == Some(true) {
            score -= penalty;
        }
    }
    r.trust_score = Some(score.max(0) as u64);
    Ok(r)
}

/// 仅取国家码，用于 DNS / UDP / 自定义站点 IP 的归属判断
pub async fn country_of(client: &reqwest::Client, cfg: &AppConfig, ip: &str) -> Option<String> {
    if !matches!(cfg.network.risk_provider.as_str(), "ipnetcoffee" | "ipapi_is") {
        return crate::detect::localdb::country(cfg, ip);
    }
    let url = cfg.network.endpoints.geoip.replace("{ip}", ip);
    let v: Value = client.get(url).send().await.ok()?.json().await.ok()?;
    let cc = s(&v, "country_code");
    (!cc.is_empty()).then(|| cc.to_uppercase())
}
