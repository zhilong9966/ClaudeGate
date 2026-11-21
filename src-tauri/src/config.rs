//! 配置模型：全局设置 + 每个受保护应用（Claude / Codex / Antigravity…）各自一份独立配置。
//! 持久化为 AppConfig 目录下的 config.json。
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const CONFIG_VERSION: u32 = 4;

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
    pub log: LogCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogCfg {
    /// 日志目录；留空在启动时填为系统日志目录
    pub dir: String,
    /// info | debug
    pub level: String,
    /// 自动删除多少天前的日志，0 = 不删除
    pub retention_days: u64,
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
    /// local（本地离线库，默认）| ipnetcoffee | ipapi_is
    pub risk_provider: String,
    pub endpoints: Endpoints,
    pub local_db: LocalDbCfg,
}

/// 本地离线 IP 库：只下载数据文件，查询在本机完成，用户 IP 不外发
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalDbCfg {
    /// 数据目录；留空在启动时填为应用数据目录下的 geo
    pub dir: String,
    /// 自动更新间隔（天），0 = 不自动更新
    pub update_interval_days: u64,
    /// 国家库（mmdb.gz），{yyyy}/{mm} 会替换为年月
    pub country_url: String,
    /// ASN 库（mmdb.gz）
    pub asn_url: String,
    /// 机房 IP 段
    pub datacenter_list_url: String,
    /// VPN IP 段
    pub vpn_list_url: String,
    /// Tor 出口节点
    pub tor_list_url: String,
    /// 恶意 / 滥用 IP 段
    pub abuse_list_url: String,
    /// ASN 组织名包含这些词即视为机房 / 云服务（补充 IP 段列表漏掉的 CDN、云厂商）
    pub hosting_asn_keywords: Vec<String>,
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
    /// 命令行包含任意一个即命中（不区分大小写），为空表示不限。用于 npm 安装的 CLI（进程本身是 node）
    pub cmd_contains: Vec<String>,
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
        Self { auto_check_on_open: false, auto_launch_when_pass: false, kill_apps_on_exit: false, guard: Guard::default(), log: LogCfg::default() }
    }
}

impl Default for LogCfg {
    fn default() -> Self {
        Self { dir: String::new(), level: "info".into(), retention_days: 14 }
    }
}

impl Default for LocalDbCfg {
    fn default() -> Self {
        Self {
            dir: String::new(),
            update_interval_days: 7,
            country_url: "https://download.db-ip.com/free/dbip-country-lite-{yyyy}-{mm}.mmdb.gz".into(),
            asn_url: "https://download.db-ip.com/free/dbip-asn-lite-{yyyy}-{mm}.mmdb.gz".into(),
            datacenter_list_url: "https://raw.githubusercontent.com/X4BNet/lists_vpn/main/output/datacenter/ipv4.txt".into(),
            vpn_list_url: "https://raw.githubusercontent.com/X4BNet/lists_vpn/main/output/vpn/ipv4.txt".into(),
            tor_list_url: "https://check.torproject.org/torbulkexitlist".into(),
            abuse_list_url: "https://raw.githubusercontent.com/firehol/blocklist-ipsets/master/firehol_level1.netset".into(),
            hosting_asn_keywords: strs(&[
                "hosting", "datacenter", "data center", "server", "vps", "colo", "cloud",
                "Amazon", "Google", "Microsoft", "Cloudflare", "Akamai", "Fastly", "Oracle", "DigitalOcean",
                "Linode", "Vultr", "Choopa", "OVH", "Hetzner", "Contabo", "Leaseweb", "M247", "Alibaba",
                "Tencent", "Huawei Cloud", "DMIT", "BandwagonHost", "IT7", "Psychz", "QuadraNet", "Zenlayer",
            ]),
        }
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
            risk_provider: "local".into(),
            endpoints: Endpoints::default(),
            local_db: LocalDbCfg::default(),
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
    ProcessRule { name: name.into(), path_contains: strs(paths), cmd_contains: vec![] }
}

fn crule(name: &str, cmds: &[&str]) -> ProcessRule {
    ProcessRule { name: name.into(), path_contains: vec![], cmd_contains: strs(cmds) }
}

/// 独立安装的 Claude Code CLI（WinGet / 官方安装脚本 / npm）
fn claude_cli_rules_windows() -> Vec<ProcessRule> {
    vec![
        prule("claude.exe", &["WinGet\\Packages\\Anthropic.ClaudeCode", "\\.local\\bin\\", "\\.local\\share\\claude\\", "\\.claude\\local\\"]),
        crule("node.exe", &["@anthropic-ai\\claude-code", "@anthropic-ai/claude-code"]),
    ]
}

/// 独立安装的 Claude Code CLI（官方安装脚本 / npm；Homebrew 路径含 claude-code，已被原规则覆盖）
fn claude_cli_rules_macos() -> Vec<ProcessRule> {
    vec![
        prule("claude", &["/.local/share/claude/", "/.local/bin/", "/.claude/local/"]),
        crule("node", &["@anthropic-ai/claude-code"]),
    ]
}

const CLAUDE_CLI_MARKERS: &[&str] = &["claude-code", "Anthropic.ClaudeCode", ".local", "@anthropic-ai"];

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
            windows_rules: [
                vec![
                    prule("Claude.exe", &["WindowsApps\\Claude_", "AnthropicClaude"]),
                    prule("claude.exe", &["Claude-Data", "claude-code"]),
                    prule("Squirrel.exe", &["AnthropicClaude"]),
                ],
                claude_cli_rules_windows(),
            ]
            .concat(),
            macos_rules: [
                vec![
                    prule("", &["/Applications/Claude.app/"]),
                    prule("ShipIt", &["com.anthropic"]),
                    prule("claude", &["claude-code"]),
                ],
                claude_cli_rules_macos(),
            ]
            .concat(),
            // 关闭 Claude 时连同独立运行的 CLI 一起结束
            keep_standalone_cli: false,
            cli_markers: strs(CLAUDE_CLI_MARKERS),
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
    /// 读取配置；v1 直接换成默认值，v2 迁移到 v3；缺失的检测项用默认值补齐
    pub fn load(path: &PathBuf) -> Self {
        let mut cfg: AppConfig = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .filter(|c: &AppConfig| c.version >= 2)
            .unwrap_or_default();
        if cfg.version == 2 {
            cfg.migrate_v2();
        }
        if cfg.version == 3 {
            // v3 → v4：IP 属性与评分改用本地离线库（DNS 泄露仍用在线检测）
            cfg.network.risk_provider = "local".into();
            cfg.version = 4;
        }
        for app in &mut cfg.apps {
            for (k, v) in default_checks(BLOCKED) {
                app.checks.entry(k).or_insert(v);
            }
        }
        cfg
    }

    /// v2 → v3：Claude 的关闭规则加入独立 CLI，保留用户已有的其他改动
    fn migrate_v2(&mut self) {
        if let Some(claude) = self.apps.iter_mut().find(|a| a.id == "claude") {
            let p = &mut claude.process;
            let same = |x: &ProcessRule, r: &ProcessRule| x.name == r.name && x.path_contains == r.path_contains && x.cmd_contains == r.cmd_contains;
            for r in claude_cli_rules_windows() {
                if !p.windows_rules.iter().any(|x| same(x, &r)) {
                    p.windows_rules.push(r);
                }
            }
            for r in claude_cli_rules_macos() {
                if !p.macos_rules.iter().any(|x| same(x, &r)) {
                    p.macos_rules.push(r);
                }
            }
            for m in CLAUDE_CLI_MARKERS {
                if !p.cli_markers.iter().any(|x| x == m) {
                    p.cli_markers.push(m.to_string());
                }
            }
            p.keep_standalone_cli = false;
        }
        self.version = 3;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_v2_adds_cli_rules_and_keeps_user_rules() {
        let mut cfg = AppConfig::default();
        cfg.version = 2;
        cfg.network.risk_provider = "ipnetcoffee".into();
        let claude = cfg.apps.iter_mut().find(|a| a.id == "claude").unwrap();
        claude.process.windows_rules.truncate(3); // v2 时只有桌面版规则
        claude.process.windows_rules.push(prule("custom.exe", &["X"])); // 用户自己加的
        claude.process.keep_standalone_cli = true;
        cfg.migrate_v2();
        let p = &cfg.app("claude").unwrap().process;
        assert_eq!(cfg.version, 3);
        assert!(!p.keep_standalone_cli);
        assert!(p.windows_rules.iter().any(|r| r.name == "custom.exe"));
        assert!(p.windows_rules.iter().any(|r| r.path_contains.iter().any(|s| s.contains("Anthropic.ClaudeCode"))));
        assert!(p.windows_rules.iter().any(|r| r.name == "node.exe"));
        // 再迁移一次不会重复添加
        let n = p.windows_rules.len();
        cfg.version = 2;
        cfg.migrate_v2();
        assert_eq!(cfg.app("claude").unwrap().process.windows_rules.len(), n);
    }
}
