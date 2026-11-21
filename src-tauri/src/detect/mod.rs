//! 检测编排：并发收集原始数据（Facts），再交给 rules.rs 判定。
pub mod availability;
pub mod cn_ip;
pub mod custom;
pub mod device;
pub mod dns_leak;
pub mod egress;
pub mod iprisk;
pub mod localdb;
pub mod status;
pub mod stun;
pub mod trace;

use crate::config::{AppConfig, AppProfile};
use serde::Serialize;

/// 一个应用的全部检测原始数据，前端直接用来渲染卡片
#[derive(Debug, Clone, Serialize, Default)]
pub struct Facts {
    pub app_id: String,
    pub app_name: String,
    pub cn_ip: Option<cn_ip::CnIp>,
    pub cf: Option<trace::Trace>,
    pub egress: Option<egress::Egress>,
    pub egress_error: Option<String>,
    pub risk: Option<iprisk::Risk>,
    pub avail: Vec<availability::Avail>,
    pub status: Option<status::Status>,
    pub dns: Option<dns_leak::DnsLeak>,
    pub udp: Option<stun::Udp>,
    pub device: device::Device,
    pub custom: Vec<custom::SiteFacts>,
}

pub async fn collect(cfg: &AppConfig, app: &AppProfile) -> Facts {
    let base = Facts { app_id: app.id.clone(), app_name: app.name.clone(), device: device::collect(), ..Default::default() };
    let client = match crate::http::client(&cfg.network) {
        Ok(c) => c,
        Err(e) => return Facts { egress_error: Some(format!("HTTP 客户端创建失败: {e}")), ..base },
    };
    let ep = &cfg.network.endpoints;
    let on = |id: &str| app.rule(id).is_some();
    let local = !matches!(cfg.network.risk_provider.as_str(), "ipnetcoffee" | "ipapi_is");
    log::info!("[{}] 开始检测（IP 数据源: {}）", app.id, if local { "本地离线库" } else { cfg.network.risk_provider.as_str() });

    // 第一轮：互不依赖的检测并发执行
    let (_, cn, cf, avail, st, dns, udp) = tokio::join!(
        // 本地离线库缺失或过期时先下载（只下载数据文件，不发送任何 IP）
        async { if local { localdb::update(&client, &cfg.network.local_db, false).await } else { vec![] } },
        cn_ip::fetch(&client, &ep.cn_ip),
        trace::fetch(&client, &ep.cf_trace),
        async {
            if on("avail") {
                futures::future::join_all(app.probes.iter().map(|p| availability::probe(&client, &p.name, &p.url))).await
            } else {
                vec![]
            }
        },
        async { if on("service_status") && !app.status_url.is_empty() { status::fetch(&client, &app.status_url).await.ok() } else { None } },
        async { if on("dns_leak") { dns_leak::detect(&client, cfg, app).await.map_err(|e| log::warn!("[{}] DNS 泄露检测失败: {e}", app.id)).ok() } else { None } },
        async { if on("udp_leak") { stun::detect(&ep.stun, cfg.network.timeout_ms).await.map_err(|e| log::warn!("[{}] UDP 出口检测失败: {e}", app.id)).ok() } else { None } },
    );
    let cf = cf.ok();

    // 第二轮：应用出口 IP（google_ecs 模式依赖 CF 出口）
    let (egress, egress_error) = match egress::fetch(&client, &app.egress, cf.as_ref()).await {
        Ok(e) => (Some(e), None),
        Err(e) => {
            log::warn!("[{}] 出口 IP 获取失败: {e}", app.id);
            (None, Some(e.to_string()))
        }
    };

    // 第三轮：依赖出口 IP 的检测
    let risk = match &egress {
        Some(e) => match iprisk::fetch(&client, cfg, &e.ip).await {
            Ok(r) => Some(r),
            Err(err) => {
                log::warn!("[{}] IP 属性查询失败: {err}", app.id);
                None
            }
        },
        None => None,
    };
    let mut egress = egress;
    if let (Some(e), Some(r)) = (egress.as_mut(), risk.as_ref()) {
        if e.loc.is_empty() {
            e.loc = r.country_code.clone();
        }
    }
    let custom = custom::collect(&client, cfg, app).await;
    let mut udp = udp;
    if let Some(u) = udp.as_mut() {
        u.country_code = iprisk::country_of(&client, cfg, &u.ip).await.unwrap_or_default();
    }

    Facts {
        cn_ip: cn.ok(),
        cf,
        egress,
        egress_error,
        risk,
        avail,
        status: st,
        dns,
        udp,
        custom,
        ..base
    }
}
