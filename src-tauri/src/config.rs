//! 配置模型：所有选项都可在设置页修改，持久化为 AppConfig 目录下的 config.json。
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub version: u32,
    pub general: General,
    pub launcher: Launcher,
    pub network: Network,
    /// 检测项规则，key = 检测 ID（见 rules.rs）
    pub checks: BTreeMap<String, CheckRule>,
    pub custom_sites: Vec<CustomSite>,
    pub process: ProcessCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub auto_check_on_open: bool,
    pub auto_launch_when_pass: bool,
    pub kill_claude_on_exit: bool,
    pub guard: Guard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Guard {
    pub enabled: bool,
    pub interval_sec: u64,
    pub kill_on_ip_change: bool,
    pub kill_on_fail: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Launcher {
    /// auto | aumid | exe
    pub windows_mode: String,
    pub windows_aumid: String,
    pub windows_exe_path: String,
    pub macos_bundle_id: String,
    pub macos_app_path: String,
    pub recheck_before_launch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Network {
    pub timeout_ms: u64,
    /// 留空 = 跟随系统；例如 http://127.0.0.1:7890 或 socks5://127.0.0.1:7891
    pub proxy: String,
    pub user_agent: String,
    /// ipnetcoffee | ipapi_is
    pub risk_provider: String,
    pub endpoints: Endpoints,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Endpoints {
    pub cf_trace: String,
    pub claude_trace: String,
    pub anthropic_trace: String,
    pub cn_ip: Vec<String>,
    pub status: String,
    pub stun: Vec<String>,
    pub dns_leak_base: String,
    pub dns_leak_result: String,
    pub iprisk_ipnetcoffee: String,
    pub iprisk_ipapi_is: String,
    pub geoip: String,
}

/// 单个检测项规则：enabled / required 固定，其余参数放在 params 里（阈值、国家列表等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRule {
    pub enabled: bool,
    pub required: bool,
    #[serde(flatten)]
    pub params: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomSite {
    pub name: String,
    pub domain: String,
    /// cf_trace | ip_echo | none
    pub egress_mode: String,
    /// ip_echo 模式下的 URL，返回文本或 JSON
    pub egress_url: String,
    /// ip_echo 返回 JSON 时的字段名（如 "ip"），为空则用正则提取 IPv4
    pub egress_json_field: String,
    /// 是否解析入口 IP（DNS）
    pub resolve_ingress: bool,
    pub required: bool,
    pub egress_same_as_claude: bool,
    pub egress_countries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProcessRule {
    /// 进程名（不区分大小写），为空表示不限
    pub name: String,
    /// 可执行路径包含任意一个即命中（不区分大小写），为空表示不限
    pub path_contains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessCfg {
    pub windows_rules: Vec<ProcessRule>,
    pub macos_rules: Vec<ProcessRule>,
    pub kill_children: bool,
    /// 保留用户在终端独立运行的 Claude Code CLI（父进程不是 Claude 桌面版）
    pub keep_standalone_cli: bool,
    pub graceful_timeout_ms: u64,
    pub verify_rounds: u32,
}

// ---------------------------------------------------------------- defaults

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            general: General::default(),
            launcher: Launcher::default(),
            network: Network::default(),
            checks: default_checks(),
            custom_sites: vec![],
            process: ProcessCfg::default(),
        }
    }
}

impl Default for General {
    fn default() -> Self {
        Self { auto_check_on_open: false, auto_launch_when_pass: false, kill_claude_on_exit: false, guard: Guard::default() }
    }
}

impl Default for Guard {
    fn default() -> Self {
        Self { enabled: true, interval_sec: 30, kill_on_ip_change: true, kill_on_fail: true }
    }
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            windows_mode: "auto".into(),
            windows_aumid: "Claude_pzs8sxrjxfjjc!Claude".into(),
            windows_exe_path: String::new(),
            macos_bundle_id: "com.anthropic.claudefordesktop".into(),
            macos_app_path: "/Applications/Claude.app".into(),
            recheck_before_launch: true,
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Self {
            timeout_ms: 8000,
            proxy: String::new(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".into(),
            risk_provider: "ipnetcoffee".into(),
            endpoints: Endpoints::default(),
        }
    }
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            cf_trace: "https://1.1.1.1/cdn-cgi/trace".into(),
            claude_trace: "https://claude.ai/cdn-cgi/trace".into(),
            anthropic_trace: "https://www.anthropic.com/cdn-cgi/trace".into(),
            cn_ip: vec!["https://my.ip.cn/".into(), "https://2026.ip138.com/".into()],
            status: "https://status.claude.com/api/v2/status.json".into(),
            stun: vec!["stun.cloudflare.com:3478".into(), "stun.l.google.com:19302".into()],
            dns_leak_base: "d.ip.net.coffee".into(),
            dns_leak_result: "https://ip.net.coffee/api/dns/result/{token}".into(),
            iprisk_ipnetcoffee: "https://ip.net.coffee/api/iprisk/{ip}".into(),
            iprisk_ipapi_is: "https://api.ipapi.is/?q={ip}".into(),
            geoip: "https://ip.net.coffee/api/geoip/{ip}".into(),
        }
    }
}

impl Default for CustomSite {
    fn default() -> Self {
        Self {
            name: String::new(),
            domain: String::new(),
            egress_mode: "cf_trace".into(),
            egress_url: String::new(),
            egress_json_field: String::new(),
            resolve_ingress: true,
            required: false,
            egress_same_as_claude: false,
            egress_countries: vec![],
        }
    }
}

impl Default for ProcessCfg {
    fn default() -> Self {
        let r = |name: &str, paths: &[&str]| ProcessRule {
            name: name.into(),
            path_contains: paths.iter().map(|s| s.to_string()).collect(),
        };
        Self {
            windows_rules: vec![
                r("Claude.exe", &["WindowsApps\\Claude_", "AnthropicClaude"]),
                r("claude.exe", &["Claude-Data", "claude-code"]),
                r("Squirrel.exe", &["AnthropicClaude"]),
            ],
            macos_rules: vec![
                r("", &["/Applications/Claude.app/"]),
                r("ShipIt", &["com.anthropic"]),
                r("claude", &["claude-code"]),
            ],
            kill_children: true,
            keep_standalone_cli: true,
            graceful_timeout_ms: 1500,
            verify_rounds: 3,
        }
    }
}

fn rule(enabled: bool, required: bool, params: Value) -> CheckRule {
    CheckRule { enabled, required, params: params.as_object().cloned().unwrap_or_default() }
}

pub fn default_checks() -> BTreeMap<String, CheckRule> {
    let mut m = BTreeMap::new();
    m.insert("claude_ip".into(), rule(true, true, json!({})));
    m.insert(
        "region".into(),
        rule(true, true, json!({
            "allowed_countries": [],
            "blocked_countries": ["CN","HK","MO","RU","IR","KP","SY","CU","BY","AF","VE"]
        })),
    );
    m.insert("trust_score".into(), rule(true, true, json!({ "min": 80, "fail_when_no_data": false })));
    m.insert("ip_type".into(), rule(true, false, json!({ "require_residential": true })));
    m.insert("vpn".into(), rule(true, false, json!({})));
    m.insert("proxy".into(), rule(true, false, json!({})));
    m.insert("tor".into(), rule(true, true, json!({})));
    m.insert("abuser".into(), rule(true, true, json!({})));
    m.insert("crawler".into(), rule(true, false, json!({})));
    m.insert("avail_claude".into(), rule(true, true, json!({ "max_latency_ms": 3000 })));
    m.insert("avail_anthropic".into(), rule(true, false, json!({ "max_latency_ms": 5000 })));
    m.insert("service_status".into(), rule(true, false, json!({ "fail_on": ["major", "critical"] })));
    m.insert("dns_leak".into(), rule(true, true, json!({ "blocked_countries": ["CN"], "method": "both" })));
    m.insert("udp_leak".into(), rule(true, false, json!({ "must_equal_claude_ip": true })));
    m.insert("ip_consistent".into(), rule(true, false, json!({ "cf_equals_claude": false })));
    m.insert("timezone".into(), rule(true, false, json!({ "mode": "same_country" })));
    m.insert("language".into(), rule(false, false, json!({ "forbid_primary": ["zh-CN"] })));
    m
}

impl AppConfig {
    /// 读取配置；缺失的检测项用默认值补齐（升级时新增的检测项也能自动出现）
    pub fn load(path: &PathBuf) -> Self {
        let mut cfg: AppConfig = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        for (k, v) in default_checks() {
            cfg.checks.entry(k).or_insert(v);
        }
        cfg
    }

    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn rule(&self, id: &str) -> Option<&CheckRule> {
        self.checks.get(id).filter(|r| r.enabled)
    }
}

impl CheckRule {
    pub fn u64(&self, key: &str, default: u64) -> u64 {
        self.params.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
    }
    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.params.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }
    pub fn str(&self, key: &str, default: &str) -> String {
        self.params.get(key).and_then(|v| v.as_str()).unwrap_or(default).to_string()
    }
    /// 字符串数组，统一转大写（国家码）或原样
    pub fn list(&self, key: &str) -> Vec<String> {
        self.params
            .get(key)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default()
    }
}
