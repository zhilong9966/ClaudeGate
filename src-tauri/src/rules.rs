//! 判定：Facts + 应用配置 → 每项 Pass/Warn/Fail，并得出 can_launch。
use crate::config::{AppProfile, CheckRule};
use crate::detect::Facts;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub required: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub facts: Facts,
    pub checks: Vec<CheckResult>,
    pub can_launch: bool,
    pub checked_at: u64,
}

struct Ctx<'a> {
    app: &'a AppProfile,
    out: Vec<CheckResult>,
}

impl<'a> Ctx<'a> {
    /// 检测项启用时执行 f；f 返回 (是否通过, 说明)。None 表示无数据。
    fn check(&mut self, id: &str, title: &str, f: impl FnOnce(&CheckRule) -> Option<(bool, String)>) {
        let Some(rule) = self.app.rule(id) else { return };
        let (status, message) = match f(rule) {
            Some((true, m)) => (Status::Pass, m),
            Some((false, m)) => (if rule.required { Status::Fail } else { Status::Warn }, m),
            None => {
                let strict = rule.bool("fail_when_no_data", false);
                (if strict && rule.required { Status::Fail } else { Status::Warn }, "无数据".into())
            }
        };
        self.out.push(CheckResult { id: id.into(), title: title.into(), status, required: rule.required, message });
    }

    fn push(&mut self, id: String, title: String, ok: bool, required: bool, message: String) {
        let status = if ok { Status::Pass } else if required { Status::Fail } else { Status::Warn };
        self.out.push(CheckResult { id, title, status, required, message });
    }
}

fn upper(v: Vec<String>) -> Vec<String> {
    v.into_iter().map(|s| s.to_uppercase()).collect()
}

pub fn evaluate(app: &AppProfile, f: Facts) -> Report {
    let mut c = Ctx { app, out: vec![] };
    let name = app.name.clone();
    let egress_ip = f.egress.as_ref().map(|t| t.ip.clone());
    let egress_cc = f
        .risk
        .as_ref()
        .map(|r| r.country_code.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| f.egress.as_ref().map(|t| t.loc.clone()).filter(|s| !s.is_empty()));

    // 出口 IP 获取失败视为硬失败（无法判断线路）
    c.check("egress_ip", &format!("{name} 出口 IP"), |_| {
        Some(match &f.egress {
            Some(t) if !t.vendor_subnet.is_empty() => (true, format!("{} ({}) · 厂商侧网段 {}", t.ip, t.loc, t.vendor_subnet)),
            Some(t) => (true, format!("{} ({})", t.ip, t.loc)),
            None => (false, format!("获取失败: {}", f.egress_error.clone().unwrap_or_default())),
        })
    });

    c.check("region", &format!("{name} 支持地区"), |r| {
        let cc = egress_cc.clone()?;
        let allowed = upper(r.list("allowed_countries"));
        let blocked = upper(r.list("blocked_countries"));
        if blocked.contains(&cc) {
            return Some((false, format!("{cc} 属于受限地区")));
        }
        if !allowed.is_empty() && !allowed.contains(&cc) {
            return Some((false, format!("{cc} 不在允许列表")));
        }
        Some((true, cc))
    });

    let risk = f.risk.clone();
    c.check("trust_score", "信任评分", |r| {
        let s = risk.as_ref()?.trust_score?;
        let min = r.u64("min", 80);
        Some((s >= min, format!("{s} 分（要求 ≥ {min}）")))
    });
    c.check("ip_type", "IP 属性", |r| {
        let rk = risk.as_ref()?;
        let res = rk.is_residential.unwrap_or(false) && rk.is_datacenter != Some(true);
        let label = if res { "住宅 IP" } else { "机房 / 非住宅 IP" };
        Some((!r.bool("require_residential", true) || res, format!("{label} · AS{} {}", rk.asn.unwrap_or(0), rk.as_org)))
    });
    for (id, title, flag) in [
        ("vpn", "VPN", risk.as_ref().and_then(|r| r.is_vpn)),
        ("proxy", "代理 (Proxy)", risk.as_ref().and_then(|r| r.is_proxy)),
        ("tor", "Tor", risk.as_ref().and_then(|r| r.is_tor)),
        ("crawler", "机器人 (Crawler)", risk.as_ref().and_then(|r| r.is_crawler)),
        ("abuser", "滥用记录", risk.as_ref().and_then(|r| r.is_abuser)),
    ] {
        // 数据源本身不提供这一项（如本地离线库无法识别普通代理/爬虫）时跳过，不算"无数据"
        if risk.is_some() && flag.is_none() {
            continue;
        }
        c.check(id, title, |_| flag.map(|v| (!v, if v { "检测到".into() } else { "未检测到".into() })));
    }

    // 可用性：每个探测地址一项；硬性 = 检测项硬性 且 该地址标记为必需
    if let Some(rule) = app.rule("avail") {
        let rule_required = rule.required;
        for p in &app.probes {
            let Some(a) = f.avail.iter().find(|a| a.name == p.name) else { continue };
            let (ok, msg) = if !a.ok {
                (false, format!("不可访问: {}", a.error))
            } else {
                (a.latency_ms <= p.max_latency_ms, format!("{} ms（上限 {} ms）", a.latency_ms, p.max_latency_ms))
            };
            c.push(format!("avail:{}", p.name), format!("{} 可用性", p.name), ok, rule_required && p.required, msg);
        }
    }

    // 没配置状态页（如 Google 无公开 Statuspage）时跳过
    if !app.status_url.is_empty() {
        c.check("service_status", &format!("{name} 服务状态"), |r| {
            let s = f.status.as_ref()?;
            let bad = r.list("fail_on").contains(&s.indicator);
            Some((!bad, s.description.clone()))
        });
    }

    c.check("dns_leak", "DNS 泄露", |r| {
        let d = f.dns.as_ref()?;
        let blocked = upper(r.list("blocked_countries"));
        let leaked: Vec<_> = d.servers.iter().filter(|s| blocked.contains(&s.country_code)).map(|s| s.ip.clone()).collect();
        let all: Vec<_> = d.servers.iter().map(|s| format!("{}({})", s.ip, s.country_code)).collect();
        Some((leaked.is_empty(), if leaked.is_empty() { format!("未泄露: {}", all.join(", ")) } else { format!("泄露到: {}", leaked.join(", ")) }))
    });

    c.check("udp_leak", "WebRTC/UDP 泄露", |r| {
        let u = f.udp.as_ref()?;
        if r.bool("must_equal_claude_ip", true) {
            let ip = egress_ip.clone()?;
            return Some((u.ip == ip, format!("UDP 出口 {}（{name} 出口 {ip}）", u.ip)));
        }
        Some((true, format!("UDP 出口 {}", u.ip)))
    });

    c.check("ip_consistent", "出口一致性", |r| {
        let ip = egress_ip.clone()?;
        let cf = f.cf.as_ref()?;
        if r.bool("cf_equals_claude", false) {
            return Some((cf.ip == ip, format!("CF {} / {name} {ip}", cf.ip)));
        }
        let cn_same = f.cn_ip.as_ref().map(|x| x.ip == ip).unwrap_or(false);
        Some((!cn_same, if cn_same { format!("{name} 出口与国内出口相同（未走代理）") } else { format!("CF {} / {name} {ip}", cf.ip) }))
    });

    c.check("timezone", "时区一致", |r| {
        let tz = &f.device.timezone;
        let ip_tz = risk.as_ref().map(|x| x.timezone.clone()).unwrap_or_default();
        match r.str("mode", "same_country").as_str() {
            "exact" if !ip_tz.is_empty() => Some((tz == &ip_tz, format!("本机 {tz} / IP {ip_tz}"))),
            _ => {
                let local = crate::detect::device::tz_country(tz)?;
                let cc = egress_cc.clone()?;
                Some((local == cc, format!("本机 {tz}（{local}）/ IP {cc}")))
            }
        }
    });

    c.check("language", "系统语言", |r| {
        let primary = f.device.locales.first()?.clone();
        let forbid = r.list("forbid_primary");
        Some((!forbid.iter().any(|x| x.eq_ignore_ascii_case(&primary)), format!("首选语言 {primary}")))
    });

    // 自定义站点
    for (site, sf) in app.custom_sites.iter().zip(f.custom.iter()) {
        let ok = if !sf.egress_error.is_empty() {
            false
        } else {
            let same = !site.egress_same_as_claude || egress_ip.as_deref() == Some(sf.egress_ip.as_str());
            let cs = upper(site.egress_countries.clone());
            same && (cs.is_empty() || cs.contains(&sf.egress_country))
        };
        let msg = format!(
            "出口 {} {}{} · 入口 {} {}",
            sf.egress_ip,
            sf.egress_country,
            if sf.egress_error.is_empty() { String::new() } else { format!("（{}）", sf.egress_error) },
            sf.ingress_ips.join(","),
            sf.ingress_country
        );
        let title = format!("站点 {}", if site.name.is_empty() { &site.domain } else { &site.name });
        c.push(format!("custom:{}", site.domain), title, ok, site.required, msg);
    }

    let can_launch = !c.out.iter().any(|x| x.required && x.status != Status::Pass);
    Report {
        facts: f,
        checks: c.out,
        can_launch,
        checked_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    }
}
