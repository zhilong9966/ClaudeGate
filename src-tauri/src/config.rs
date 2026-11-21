//! 配置模型：全局设置 + 每个受保护应用（Claude / Codex / Antigravity…）各自一份独立配置。
//! 持久化为 AppConfig 目录下的 config.json。
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const CONFIG_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub version: u32,
    pub general: General,
    pub network: Network,
    /// 每个应用一个标签页
    pub apps: Vec<AppProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub auto_check_on_open: bool,
    pub auto_launch_when_pass: bool,
    /// 退出 ClaudeGate 时结束所有已启用应用的进程
    pub kill_apps_on_exit: bool,
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
pub struct Network {
    pub timeout_ms: u64,
    /// 留空 = 跟随系统；例如 http://127.0.0.1:7890 或 socks5://127.0.0.1:7891
    pub proxy: String,
    pub user_agent: String,
    /// ipnetcoffee | ipapi_is
    pub risk_provider: String,
    pub endpoints: Endpoints,
}

/// 所有应用共用的检测端点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Endpoints {
    pub cf_trace: String,
    pub cn_ip: Vec<String>,
    pub stun: Vec<String>,
    pub dns_leak_base: String,
    pub dns_leak_result: String,
    pub iprisk_ipnetcoffee: String,
    pub iprisk_ipapi_is: String,
    pub geoip: String,
}

// ---------------------------------------------------------------- 单个应用

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// 该应用服务端看到的出口 IP 怎么取
    pub egress: Egress,
    /// 可用性探测（只请求不需要登录的地址）
    pub probes: Vec<Probe>,
    /// Statuspage 格式的状态 JSON，留空则不检测
    pub status_url: String,
    pub launcher: Launcher,
    /// 检测项规则，key = 检测 ID（见 rules.rs）
    pub checks: BTreeMap<String, CheckRule>,
    pub custom_sites: Vec<CustomSite>,
    pub process: ProcessCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Egress {
    /// cf_trace：请求该厂商域名的 /cdn-cgi/trace（Cloudflare 边缘返回）
    /// ip_echo：请求返回 IP 的地址（文本/JSON）
    /// google_ecs：通过 dns.google 读取 Google 看到的客户端网段，再与 CF 出口 IP 比对
    pub mode: String,
    pub url: String,
    pub json_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Probe {
    pub name: String,
    pub url: String,
    pub required: bool,
    pub max_latency_ms: u64,
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
    pub egress_url: String,
    /// ip_echo 返回 JSON 时的字段名（如 "ip"），为空则用正则提取 IPv4
    pub egress_json_field: String,
    pub resolve_ingress: bool,
    pub required: bool,
    /// 出口须与本应用的出口 IP 一致
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
    /// 保留用户在终端独立运行的 CLI（祖先进程里没有桌面版）
    pub keep_standalone_cli: bool,
    /// 路径包含这些片段的进程视为 CLI
    pub cli_markers: Vec<String>,
    /// 路径包含这些片段的进程视为桌面版主程序
    pub desktop_markers: Vec<String>,
    pub graceful_timeout_ms: u64,
    pub verify_rounds: u32,
}

// ---------------------------------------------------------------- defaults

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            general: General::default(),
            network: Network::default(),
            apps: vec![claude_profile(), codex_profile(), antigravity_profile()],
        }
    }
}

impl Default for General {
    fn default() -> Self {
        Self { auto_check_on_open: false, auto_launch_when_pass: false, kill_apps_on_exit: false, guard: Guard::default() }
    }
}

impl Default for Guard {
    fn default() -> Self {
        Self { enabled: true, interval_sec: 30, kill_on_ip_change: true, kill_on_fail: true }
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
            cn_ip: vec!["https://my.ip.cn/".into(), "https://2026.ip138.com/".into()],
            stun: vec!["stun.cloudflare.com:3478".into(), "stun.l.google.com:19302".into()],
            dns_leak_base: "d.ip.net.coffee".into(),
            dns_leak_result: "https://ip.net.coffee/api/dns/result/{token}".into(),
            iprisk_ipnetcoffee: "https://ip.net.coffee/api/iprisk/{ip}".into(),
            iprisk_ipapi_is: "https://api.ipapi.is/?q={ip}".into(),
            geoip: "https://ip.net.coffee/api/geoip/{ip}".into(),
        }
    }
}

impl Default for AppProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            egress: Egress::default(),
            probes: vec![],
            status_url: String::new(),
            launcher: Launcher::default(),
            checks: default_checks(&[]),
            custom_sites: vec![],
            process: ProcessCfg::default(),
        }
    }
}

impl Default for Egress {
    fn default() -> Self {
        Self { mode: "cf_trace".into(), url: String::new(), json_field: String::new() }
    }
}

impl Default for Probe {
    fn default() -> Self {
        Self { name: String::new(), url: String::new(), required: false, max_latency_ms: 5000 }
    }
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            windows_mode: "auto".into(),
            windows_aumid: String::new(),
            windows_exe_path: String::new(),
            macos_bundle_id: String::new(),
            macos_app_path: String::new(),
            recheck_before_launch: true,
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
        Self {
            windows_rules: vec![],
            macos_rules: vec![],
            kill_children: true,
            keep_standalone_cli: true,
            cli_markers: vec![],
            desktop_markers: vec![],
            graceful_timeout_ms: 1500,
            verify_rounds: 3,
        }
    }
}

fn rule(enabled: bool, required: bool, params: Value) -> CheckRule {
    CheckRule { enabled, required, params: params.as_object().cloned().unwrap_or_default() }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn prule(name: &str, paths: &[&str]) -> ProcessRule {
    ProcessRule { name: name.into(), path_contains: strs(paths) }
}

fn probe(name: &str, url: &str, required: bool, max_latency_ms: u64) -> Probe {
    Probe { name: name.into(), url: url.into(), required, max_latency_ms }
}

/// 三家都不对中国大陆/港澳及受制裁地区提供服务
const BLOCKED: &[&str] = &["CN", "HK", "MO", "RU", "IR", "KP", "SY", "CU", "BY", "AF", "VE"];

pub fn default_checks(blocked: &[&str]) -> BTreeMap<String, CheckRule> {
    let mut m = BTreeMap::new();
    m.insert("egress_ip".into(), rule(true, true, json!({})));
    m.insert("region".into(), rule(true, true, json!({ "allowed_countries": [], "blocked_countries": blocked })));
    m.insert("trust_score".into(), rule(true, true, json!({ "min": 80, "fail_when_no_data": false })));
    m.insert("ip_type".into(), rule(true, false, json!({ "require_residential": true })));
    m.insert("vpn".into(), rule(true, false, json!({})));
    m.insert("proxy".into(), rule(true, false, json!({})));
    m.insert("tor".into(), rule(true, true, json!({})));
    m.insert("abuser".into(), rule(true, true, json!({})));
    m.insert("crawler".into(), rule(true, false, json!({})));
    m.insert("avail".into(), rule(true, true, json!({})));
    m.insert("service_status".into(), rule(true, false, json!({ "fail_on": ["major", "critical"] })));
    m.insert("dns_leak".into(), rule(true, true, json!({ "blocked_countries": ["CN"], "method": "both" })));
    m.insert("udp_leak".into(), rule(true, false, json!({ "must_equal_claude_ip": true })));
    m.insert("ip_consistent".into(), rule(true, false, json!({ "cf_equals_claude": false })));
    m.insert("timezone".into(), rule(true, false, json!({ "mode": "same_country" })));
    m.insert("language".into(), rule(false, false, json!({ "forbid_primary": ["zh-CN"] })));
    m
}

pub fn claude_profile() -> AppProfile {
    AppProfile {
        id: "claude".into(),
        name: "Claude".into(),
        egress: Egress { mode: "cf_trace".into(), url: "https://claude.ai/cdn-cgi/trace".into(), json_field: String::new() },
        probes: vec![
            probe("claude.ai", "https://claude.ai/cdn-cgi/trace", true, 3000),
            probe("anthropic.com", "https://www.anthropic.com/cdn-cgi/trace", false, 5000),
        ],
        status_url: "https://status.claude.com/api/v2/status.json".into(),
        launcher: Launcher {
            windows_aumid: "Claude_pzs8sxrjxfjjc!Claude".into(),
            macos_bundle_id: "com.anthropic.claudefordesktop".into(),
            macos_app_path: "/Applications/Claude.app".into(),
            ..Default::default()
        },
        checks: default_checks(BLOCKED),
        process: ProcessCfg {
            windows_rules: vec![
                prule("Claude.exe", &["WindowsApps\\Claude_", "AnthropicClaude"]),
                prule("claude.exe", &["Claude-Data", "claude-code"]),
                prule("Squirrel.exe", &["AnthropicClaude"]),
            ],
            macos_rules: vec![
                prule("", &["/Applications/Claude.app/"]),
                prule("ShipIt", &["com.anthropic"]),
                prule("claude", &["claude-code"]),
            ],
            cli_markers: strs(&["claude-code"]),
            desktop_markers: strs(&["WindowsApps\\Claude_", "AnthropicClaude", "/Applications/Claude.app/"]),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn codex_profile() -> AppProfile {
    AppProfile {
        id: "codex".into(),
        name: "Codex".into(),
        egress: Egress { mode: "cf_trace".into(), url: "https://chatgpt.com/cdn-cgi/trace".into(), json_field: String::new() },
        probes: vec![
            probe("chatgpt.com", "https://chatgpt.com/cdn-cgi/trace", true, 3000),
            probe("api.openai.com", "https://api.openai.com/cdn-cgi/trace", false, 5000),
            probe("auth.openai.com", "https://auth.openai.com/cdn-cgi/trace", false, 5000),
        ],
        status_url: "https://status.openai.com/api/v2/status.json".into(),
        launcher: Launcher {
            windows_aumid: "OpenAI.Codex_2p2nqsd0c76g0!App".into(),
            macos_bundle_id: "com.openai.codex".into(),
            macos_app_path: "/Applications/Codex.app".into(),
            ..Default::default()
        },
        checks: default_checks(BLOCKED),
        process: ProcessCfg {
            windows_rules: vec![prule("", &["WindowsApps\\OpenAI.Codex_", "WindowsApps\\OpenAI.CodexPrimaryRuntime"])],
            macos_rules: vec![prule("", &["/Applications/Codex.app/"])],
            desktop_markers: strs(&["WindowsApps\\OpenAI.Codex", "/Applications/Codex.app/"]),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn antigravity_profile() -> AppProfile {
    AppProfile {
        id: "antigravity".into(),
        name: "Antigravity".into(),
        egress: Egress {
            mode: "google_ecs".into(),
            url: "https://dns.google/resolve?name=o-o.myaddr.l.google.com&type=TXT".into(),
            json_field: String::new(),
        },
        probes: vec![
            probe("antigravity.google", "https://antigravity.google/", true, 3000),
            probe("cloudcode-pa.googleapis.com", "https://cloudcode-pa.googleapis.com/", false, 5000),
        ],
        status_url: String::new(),
        launcher: Launcher {
            windows_aumid: "Google.Antigravity".into(),
            macos_bundle_id: "com.google.antigravity".into(),
            macos_app_path: "/Applications/Antigravity.app".into(),
            ..Default::default()
        },
        checks: default_checks(BLOCKED),
        process: ProcessCfg {
            windows_rules: vec![prule("", &["Programs\\antigravity\\", "Programs\\Antigravity IDE\\"])],
            macos_rules: vec![prule("", &["/Applications/Antigravity.app/", "/Applications/Antigravity IDE.app/"])],
            desktop_markers: strs(&["Programs\\antigravity\\", "Programs\\Antigravity IDE\\", "/Applications/Antigravity"]),
            ..Default::default()
        },
        ..Default::default()
    }
}

impl AppConfig {
    /// 读取配置；旧版本配置直接换成默认值；缺失的检测项用默认值补齐
    pub fn load(path: &PathBuf) -> Self {
        let mut cfg: AppConfig = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .filter(|c: &AppConfig| c.version == CONFIG_VERSION)
            .unwrap_or_default();
        for app in &mut cfg.apps {
            for (k, v) in default_checks(BLOCKED) {
                app.checks.entry(k).or_insert(v);
            }
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

    pub fn app(&self, id: &str) -> Option<&AppProfile> {
        self.apps.iter().find(|a| a.id == id)
    }
}

impl AppProfile {
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
    pub fn list(&self, key: &str) -> Vec<String> {
        self.params
            .get(key)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default()
    }
}
