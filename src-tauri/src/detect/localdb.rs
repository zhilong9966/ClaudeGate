//! 本地离线 IP 库：国家 / ASN（DB-IP Lite，CC BY 4.0）+ 机房 / VPN / Tor / 滥用 IP 段列表。
//! 只从数据源下载文件，查询完全在本机完成，用户 IP 不会发给任何第三方。
//! 信任分由本地规则计算（见 score()），算法公开透明。
use crate::config::{AppConfig, LocalDbCfg};
use crate::detect::iprisk::Risk;
use chrono::{Datelike, Local};
use serde::Serialize;
use std::io::Read;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// (文件名, 说明)
const FILES: &[(&str, &str)] = &[
    ("country.mmdb", "国家库"),
    ("asn.mmdb", "ASN 库"),
    ("datacenter.txt", "机房 IP 段"),
    ("vpn.txt", "VPN IP 段"),
    ("tor.txt", "Tor 出口节点"),
    ("abuse.txt", "滥用 IP 段"),
];

#[derive(Debug, Clone, Serialize)]
pub struct FileStatus {
    pub file: String,
    pub label: String,
    pub exists: bool,
    pub size: u64,
    /// 最后更新时间（本地时间字符串）
    pub updated: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbStatus {
    pub dir: String,
    pub files: Vec<FileStatus>,
}

pub fn dir(cfg: &LocalDbCfg) -> PathBuf {
    PathBuf::from(&cfg.dir)
}

pub fn status(cfg: &LocalDbCfg) -> DbStatus {
    let d = dir(cfg);
    DbStatus {
        dir: d.to_string_lossy().into_owned(),
        files: FILES
            .iter()
            .map(|(f, label)| {
                let meta = std::fs::metadata(d.join(f)).ok();
                FileStatus {
                    file: f.to_string(),
                    label: label.to_string(),
                    exists: meta.is_some(),
                    size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    updated: meta
                        .and_then(|m| m.modified().ok())
                        .map(|t| chrono::DateTime::<Local>::from(t).format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default(),
                }
            })
            .collect(),
    }
}

fn stale(path: &Path, interval_days: u64) -> bool {
    let Ok(m) = std::fs::metadata(path).and_then(|m| m.modified()) else { return true };
    interval_days > 0 && SystemTime::now().duration_since(m).unwrap_or_default() > Duration::from_secs(interval_days * 86400)
}

/// 缺失的文件一定下载；force 或超过更新间隔的文件重新下载。返回每个文件的结果说明。
pub async fn update(client: &reqwest::Client, cfg: &LocalDbCfg, force: bool) -> Vec<String> {
    let d = dir(cfg);
    if let Err(e) = std::fs::create_dir_all(&d) {
        log::error!("创建离线库目录失败 {}: {e}", d.display());
        return vec![format!("创建目录失败: {e}")];
    }
    let jobs: Vec<(&str, &str, bool)> = vec![
        ("country.mmdb", cfg.country_url.as_str(), true),
        ("asn.mmdb", cfg.asn_url.as_str(), true),
        ("datacenter.txt", cfg.datacenter_list_url.as_str(), false),
        ("vpn.txt", cfg.vpn_list_url.as_str(), false),
        ("tor.txt", cfg.tor_list_url.as_str(), false),
        ("abuse.txt", cfg.abuse_list_url.as_str(), false),
    ];
    let mut out = vec![];
    for (file, url, is_mmdb) in jobs {
        let path = d.join(file);
        let exists = path.exists();
        if exists && !force && !stale(&path, cfg.update_interval_days) {
            continue;
        }
        let res = if is_mmdb { fetch_mmdb(client, url, &path).await } else { fetch_list(client, url, &path).await };
        let msg = match res {
            Ok(n) => {
                log::info!("离线库已更新 {file}（{n} 字节）");
                format!("{file}: 已更新（{} KB）", n / 1024)
            }
            Err(e) => {
                log::warn!("离线库更新失败 {file}: {e}{}", if exists { "，继续使用旧文件" } else { "" });
                format!("{file}: 更新失败（{e}）{}", if exists { "，继续使用旧文件" } else { "" })
            }
        };
        out.push(msg);
    }
    out
}

/// mmdb：URL 中的 {yyyy}/{mm} 依次尝试本月、上月（月初数据源可能尚未发布）
async fn fetch_mmdb(client: &reqwest::Client, tpl: &str, path: &Path) -> anyhow::Result<usize> {
    let now = Local::now().date_naive();
    let prev = now - chrono::Duration::days(now.day() as i64);
    let mut last = anyhow::anyhow!("无可用地址");
    for d in [now, prev] {
        let url = tpl.replace("{yyyy}", &format!("{:04}", d.year())).replace("{mm}", &format!("{:02}", d.month()));
        match download(client, &url).await {
            Ok(bytes) => {
                let data = if url.ends_with(".gz") {
                    let mut v = vec![];
                    flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut v)?;
                    v
                } else {
                    bytes
                };
                // 校验能被解析再落盘
                maxminddb::Reader::from_source(data.clone()).map_err(|e| anyhow::anyhow!("mmdb 格式错误: {e}"))?;
                write_atomic(path, &data)?;
                return Ok(data.len());
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

async fn fetch_list(client: &reqwest::Client, url: &str, path: &Path) -> anyhow::Result<usize> {
    let bytes = download(client, url).await?;
    let text = String::from_utf8_lossy(&bytes);
    anyhow::ensure!(!parse_ranges(&text).is_empty(), "列表内容为空或格式不对");
    write_atomic(path, &bytes)?;
    Ok(bytes.len())
}

async fn download(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<u8>> {
    // 数据文件较大，单独给更长的超时
    let r = client.get(url).timeout(Duration::from_secs(120)).send().await?.error_for_status()?;
    Ok(r.bytes().await?.to_vec())
}

fn write_atomic(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 解析 "a.b.c.d/n" 或 "a.b.c.d"，忽略注释和 IPv6
fn parse_ranges(text: &str) -> Vec<(u32, u32)> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let (ip, bits) = l.split_once('/').unwrap_or((l, "32"));
            let ip: Ipv4Addr = ip.trim().parse().ok()?;
            let bits: u32 = bits.trim().parse().ok().filter(|b| *b <= 32)?;
            let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
            let start = u32::from(ip) & mask;
            Some((start, start | !mask))
        })
        .collect()
}

fn in_list(d: &Path, file: &str, ip: Ipv4Addr) -> Option<bool> {
    let text = std::fs::read_to_string(d.join(file)).ok()?;
    let v = u32::from(ip);
    Some(parse_ranges(&text).iter().any(|(s, e)| *s <= v && v <= *e))
}

fn country_lookup(d: &Path, ip: std::net::IpAddr) -> Option<(String, String)> {
    let r = maxminddb::Reader::open_readfile(d.join("country.mmdb")).ok()?;
    let c: maxminddb::geoip2::Country = r.lookup(ip).ok()?;
    let country = c.country?;
    let code = country.iso_code.unwrap_or("").to_uppercase();
    let name = country.names.and_then(|n| n.get("en").map(|s| s.to_string())).unwrap_or_default();
    Some((code, name))
}

fn asn_lookup(d: &Path, ip: std::net::IpAddr) -> Option<(u64, String)> {
    let r = maxminddb::Reader::open_readfile(d.join("asn.mmdb")).ok()?;
    let a: maxminddb::geoip2::Asn = r.lookup(ip).ok()?;
    Some((a.autonomous_system_number? as u64, a.autonomous_system_organization.unwrap_or("").to_string()))
}

/// 仅国家码，供 DNS 解析器 / UDP 出口 / 自定义站点归属判断使用
/// IPv4 / IPv6 均可（DB-IP Lite 同时覆盖两者）
pub fn country(cfg: &AppConfig, ip: &str) -> Option<String> {
    let ip: std::net::IpAddr = ip.parse().ok()?;
    country_lookup(&dir(&cfg.network.local_db), ip).map(|(c, _)| c).filter(|c| !c.is_empty())
}

/// 本地信任分：从 100 分起扣
/// - Tor 出口 -80，滥用记录 -50，机房 / 云 IP -40，VPN -30（同时命中累计扣分，最低 0）
pub fn score(r: &Risk) -> u64 {
    let mut s: i64 = 100;
    for (flag, penalty) in [(r.is_tor, 80), (r.is_abuser, 50), (r.is_datacenter, 40), (r.is_vpn, 30)] {
        if flag == Some(true) {
            s -= penalty;
        }
    }
    s.max(0) as u64
}

pub fn lookup(cfg: &AppConfig, ip: &str) -> anyhow::Result<Risk> {
    let d = dir(&cfg.network.local_db);
    let addr: Ipv4Addr = ip.parse().map_err(|_| anyhow::anyhow!("暂只支持 IPv4: {ip}"))?;
    let (country_code, country) =
        country_lookup(&d, addr.into()).ok_or_else(|| anyhow::anyhow!("国家库缺失或查询失败（目录 {}）", d.display()))?;
    let asn = asn_lookup(&d, addr.into());
    // 机房判定：IP 段列表命中，或 ASN 组织名含云 / 机房关键词
    let org = asn.as_ref().map(|a| a.1.to_ascii_lowercase()).unwrap_or_default();
    let asn_hosting = !org.is_empty()
        && cfg.network.local_db.hosting_asn_keywords.iter().any(|k| !k.is_empty() && org.contains(&k.to_ascii_lowercase()));
    let is_datacenter = in_list(&d, "datacenter.txt", addr).map(|hit| hit || asn_hosting);
    let is_vpn = in_list(&d, "vpn.txt", addr);
    let is_tor = in_list(&d, "tor.txt", addr);
    let is_abuser = in_list(&d, "abuse.txt", addr);
    let is_residential = match (is_datacenter, is_vpn, is_tor) {
        (Some(dc), Some(v), Some(t)) => Some(!dc && !v && !t),
        _ => None,
    };
    let mut r = Risk {
        provider: "本地离线库".into(),
        ip: ip.into(),
        country,
        country_code,
        asn: asn.as_ref().map(|a| a.0),
        as_org: asn.map(|a| a.1).unwrap_or_default(),
        company_type: match is_residential {
            Some(true) => "isp".into(),
            Some(false) => "hosting".into(),
            None => String::new(),
        },
        is_residential,
        is_datacenter,
        is_vpn,
        is_proxy: None, // 离线列表无法区分普通代理，留空（显示"无数据"）
        is_tor,
        is_crawler: None,
        is_abuser,
        ..Default::default()
    };
    // 机房/VPN/Tor/滥用列表都在时才给分，否则视为无数据
    if is_datacenter.is_some() && is_vpn.is_some() && is_tor.is_some() && is_abuser.is_some() {
        r.trust_score = Some(score(&r));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        let v = parse_ranges("# c\n1.2.3.0/24\n5.6.7.8\n::1/128\nbad\n");
        assert_eq!(v, vec![(0x01020300, 0x010203FF), (0x05060708, 0x05060708)]);
    }

    #[test]
    fn scoring() {
        let clean = Risk { is_datacenter: Some(false), is_vpn: Some(false), is_tor: Some(false), is_abuser: Some(false), ..Default::default() };
        assert_eq!(score(&clean), 100);
        let dc = Risk { is_datacenter: Some(true), ..clean.clone() };
        assert_eq!(score(&dc), 60);
        let tor = Risk { is_tor: Some(true), is_abuser: Some(true), ..clean };
        assert_eq!(score(&tor), 0);
    }
}

#[cfg(test)]
mod live {
    /// 依赖 live_checks 已下载的离线库：cargo test --lib local_lookup_samples -- --ignored --nocapture
    #[test]
    #[ignore]
    fn local_lookup_samples() {
        let mut cfg = crate::config::AppConfig::default();
        let dir = std::env::temp_dir().join("claudegate-test-geo");
        cfg.network.local_db.dir = dir.to_string_lossy().into_owned();
        let tor = std::fs::read_to_string(dir.join("tor.txt")).unwrap().lines().next().unwrap().trim().to_string();
        for ip in ["66.227.98.150", "8.8.8.8", "104.16.0.1", tor.as_str()] {
            let r = super::lookup(&cfg, ip).unwrap();
            println!(
                "{ip}: {} AS{:?} {} | dc={:?} vpn={:?} tor={:?} abuse={:?} residential={:?} score={:?}",
                r.country_code, r.asn, r.as_org, r.is_datacenter, r.is_vpn, r.is_tor, r.is_abuser, r.is_residential, r.trust_score
            );
        }
    }
}
