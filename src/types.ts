export type Status = "pass" | "warn" | "fail";

export interface CheckResult { id: string; title: string; status: Status; required: boolean; message: string }
export interface Trace { ip: string; loc: string; colo: string; warp: string; host: string }
export interface Risk {
  provider: string; ip: string; country: string; country_code: string; city: string;
  asn?: number; as_org: string; company_type: string;
  is_residential?: boolean; is_datacenter?: boolean; is_vpn?: boolean; is_proxy?: boolean;
  is_tor?: boolean; is_crawler?: boolean; is_abuser?: boolean; abuser_score: string;
  timezone: string; trust_score?: number;
}
export interface Avail { name: string; ok: boolean; latency_ms: number; error: string }
export interface Facts {
  cn_ip?: { ip: string; raw: string; source: string };
  cf?: Trace; claude?: Trace; claude_error?: string; risk?: Risk;
  avail_claude?: Avail; avail_anthropic?: Avail;
  status?: { indicator: string; description: string };
  dns?: { servers: { ip: string; country_code: string }[] };
  udp?: { ip: string; server: string; country_code: string };
  device: { timezone: string; locales: string[]; os: string; arch: string };
  custom: {
    name: string; domain: string; egress_ip: string; egress_country: string; egress_error: string;
    ingress_ips: string[]; ingress_country: string; ingress_error: string;
  }[];
}
export interface Report { facts: Facts; checks: CheckResult[]; can_launch: boolean; checked_at: number }
export interface ProcInfo { pid: number; name: string; path: string }
export interface KillReport { killed: ProcInfo[]; remaining: ProcInfo[] }

export interface CheckRule { enabled: boolean; required: boolean; [k: string]: unknown }
export interface CustomSite {
  name: string; domain: string; egress_mode: string; egress_url: string; egress_json_field: string;
  resolve_ingress: boolean; required: boolean; egress_same_as_claude: boolean; egress_countries: string[];
}
export interface ProcessRule { name: string; path_contains: string[] }
export interface AppConfig {
  version: number;
  general: {
    auto_check_on_open: boolean; auto_launch_when_pass: boolean; kill_claude_on_exit: boolean;
    guard: { enabled: boolean; interval_sec: number; kill_on_ip_change: boolean; kill_on_fail: boolean };
  };
  launcher: {
    windows_mode: string; windows_aumid: string; windows_exe_path: string;
    macos_bundle_id: string; macos_app_path: string; recheck_before_launch: boolean;
  };
  network: {
    timeout_ms: number; proxy: string; user_agent: string; risk_provider: string;
    endpoints: Record<string, string | string[]>;
  };
  checks: Record<string, CheckRule>;
  custom_sites: CustomSite[];
  process: {
    windows_rules: ProcessRule[]; macos_rules: ProcessRule[]; kill_children: boolean;
    keep_standalone_cli: boolean; graceful_timeout_ms: number; verify_rounds: number;
  };
}

export const CHECK_TITLES: Record<string, string> = {
  claude_ip: "Claude 出口 IP 获取", region: "Claude 支持地区", trust_score: "信任评分",
  ip_type: "IP 属性（住宅/机房）", vpn: "VPN", proxy: "代理 (Proxy)", tor: "Tor",
  crawler: "机器人 (Crawler)", abuser: "滥用记录", avail_claude: "claude.ai 可用性",
  avail_anthropic: "anthropic.com 可用性", service_status: "Claude 服务状态",
  dns_leak: "DNS 泄露", udp_leak: "WebRTC/UDP 泄露", ip_consistent: "出口一致性",
  timezone: "时区一致", language: "系统语言",
};

export const PARAM_TITLES: Record<string, string> = {
  allowed_countries: "允许国家（空=不限）", blocked_countries: "禁止国家", min: "最低分",
  fail_when_no_data: "无数据时判失败", require_residential: "必须住宅 IP", max_latency_ms: "最大延迟 ms",
  fail_on: "以下状态判失败", must_equal_claude_ip: "UDP 出口须等于 Claude 出口",
  cf_equals_claude: "CF 出口须等于 Claude 出口", mode: "模式 (same_country / exact)",
  forbid_primary: "禁止的首选语言", method: "解析方式 (both / system / http)",
};
