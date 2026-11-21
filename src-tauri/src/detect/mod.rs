//! 检测编排：并发收集原始数据（Facts），再交给 rules.rs 判定。
pub mod availability;
pub mod cn_ip;
pub mod custom;
pub mod device;
pub mod dns_leak;
pub mod iprisk;
pub mod status;
pub mod stun;
pub mod trace;

use crate::config::AppConfig;
use serde::Serialize;

/// 所有检测的原始数据，前端直接用来渲染卡片
#[derive(Debug, Clone, Serialize, Default)]
pub struct Facts {
    pub cn_ip: Option<cn_ip::CnIp>,
    pub cf: Option<trace::Trace>,
    pub claude: Option<trace::Trace>,
    pub claude_error: Option<String>,
    pub risk: Option<iprisk::Risk>,
    pub avail_claude: Option<availability::Avail>,
    pub avail_anthropic: Option<availability::Avail>,
    pub status: Option<status::Status>,
    pub dns: Option<dns_leak::DnsLeak>,
    pub udp: Option<stun::Udp>,
    pub device: device::Device,
    pub custom: Vec<custom::SiteFacts>,
}

pub async fn collect(cfg: &AppConfig) -> Facts {
    let client = match crate::http::client(&cfg.network) {
        Ok(c) => c,
        Err(e) => {
            return Facts { claude_error: Some(format!("HTTP 客户端创建失败: {e}")), device: device::collect(), ..Default::default() }
        }
    };
    let ep = &cfg.network.endpoints;
    let on = |id: &str| cfg.rule(id).is_some();

    // 第一轮：互不依赖的检测并发执行
    let (cn, cf, claude, a_claude, a_anth, st, dns, udp) = tokio::join!(
        cn_ip::fetch(&client, &ep.cn_ip),
        trace::fetch(&client, &ep.cf_trace),
        trace::fetch(&client, &ep.claude_trace),
        async { if on("avail_claude") { Some(availability::probe(&client, "claude.ai", &ep.claude_trace).await) } else { None } },
        async { if on("avail_anthropic") { Some(availability::probe(&client, "anthropic.com", &ep.anthropic_trace).await) } else { None } },
        async { if on("service_status") { status::fetch(&client, &ep.status).await.ok() } else { None } },
        async { if on("dns_leak") { dns_leak::detect(&client, cfg).await.ok() } else { None } },
        async { if on("udp_leak") { stun::detect(&ep.stun, cfg.network.timeout_ms).await.ok() } else { None } },
    );

    let (claude, claude_error) = match claude {
        Ok(t) => (Some(t), None),
        Err(e) => (None, Some(e.to_string())),
    };

    // 第二轮：依赖 Claude 出口 IP 的检测
    let risk = match &claude {
        Some(t) => iprisk::fetch(&client, cfg, &t.ip).await.ok(),
        None => None,
    };
    let custom = custom::collect(&client, cfg).await;
    let mut udp = udp;
    if let Some(u) = udp.as_mut() {
        u.country_code = iprisk::country_of(&client, cfg, &u.ip).await.unwrap_or_default();
    }

    Facts {
        cn_ip: cn.ok(),
        cf: cf.ok(),
        claude,
        claude_error,
        risk,
        avail_claude: a_claude,
        avail_anthropic: a_anth,
        status: st,
        dns,
        udp,
        device: device::collect(),
        custom,
    }
}
