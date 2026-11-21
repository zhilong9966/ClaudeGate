//! 本机环境：时区、语言、系统。用于和 IP 归属地做一致性比对。
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Device {
    pub timezone: String,
    pub locales: Vec<String>,
    pub os: String,
    pub arch: String,
}

pub fn collect() -> Device {
    Device {
        timezone: iana_time_zone::get_timezone().unwrap_or_default(),
        locales: sys_locale::get_locales().collect(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
    }
}

/// 粗略的时区 → 国家映射（只覆盖常见地区；未知时区返回 None，判定为 Warn）
pub fn tz_country(tz: &str) -> Option<&'static str> {
    let t = tz.to_ascii_lowercase();
    let map: &[(&str, &str)] = &[
        ("america/new_york", "US"), ("america/chicago", "US"), ("america/denver", "US"),
        ("america/los_angeles", "US"), ("america/phoenix", "US"), ("america/anchorage", "US"),
        ("america/detroit", "US"), ("pacific/honolulu", "US"), ("america/indiana", "US"),
        ("america/toronto", "CA"), ("america/vancouver", "CA"), ("europe/london", "GB"),
        ("europe/berlin", "DE"), ("europe/paris", "FR"), ("europe/amsterdam", "NL"),
        ("asia/tokyo", "JP"), ("asia/seoul", "KR"), ("asia/singapore", "SG"),
        ("asia/taipei", "TW"), ("asia/hong_kong", "HK"), ("asia/shanghai", "CN"),
        ("asia/chongqing", "CN"), ("asia/urumqi", "CN"), ("australia/sydney", "AU"),
        ("australia/melbourne", "AU"), ("asia/kolkata", "IN"), ("europe/madrid", "ES"),
        ("europe/rome", "IT"), ("europe/zurich", "CH"), ("europe/stockholm", "SE"),
    ];
    map.iter().find(|(k, _)| t.starts_with(k)).map(|(_, v)| *v)
}
