# ClaudeGate 架构设计

> 一个跨平台（Windows / macOS）的 Claude 桌面版"安全启动器"：
> **启动前**检测网络环境，硬性条件全部通过后才出现"启动 Claude"按钮；
> **关闭时**彻底杀掉 Claude 及其所有子进程/后台进程。

---

## 0. 多应用（v2 配置）

ClaudeGate 同时保护多个 AI 桌面应用，每个应用一个顶部标签页，检测、启动、关闭、守护、配置完全独立：

| 应用 | 出口 IP 获取 | 可用性探测 | 状态页 | 启动（Windows） | 进程规则（Windows） |
|---|---|---|---|---|---|
| Claude | `claude.ai/cdn-cgi/trace` | claude.ai★、anthropic.com | status.claude.com | AUMID `Claude_pzs8sxrjxfjjc!Claude` | `WindowsApps\Claude_`、`Claude-Data`/`claude-code` |
| Codex | `chatgpt.com/cdn-cgi/trace` | chatgpt.com★、api.openai.com、auth.openai.com | status.openai.com | AUMID `OpenAI.Codex_2p2nqsd0c76g0!App` | `WindowsApps\OpenAI.Codex_`、`OpenAI.CodexPrimaryRuntime` |
| Antigravity | `google_ecs`：dns.google 返回 Google 看到的客户端网段，再与 CF 出口 IP 比对 | antigravity.google★、cloudcode-pa.googleapis.com | 无（跳过） | AUMID `Google.Antigravity` | `Programs\antigravity\`、`Programs\Antigravity IDE\` |

- Google 系没有 `/cdn-cgi/trace`，因此 Antigravity 用 `https://dns.google/resolve?name=o-o.myaddr.l.google.com&type=TXT`：
  返回的 `edns0-client-subnet` 是 **Google 侧看到的你的 /24 网段**，与 CF 出口 IP 不在同一网段即判定"分流规则不同"→ 硬失败。
- 配置结构：`{ version: 2, general, network, apps: [AppProfile…] }`，`AppProfile` 含 `egress / probes / status_url / launcher / checks / custom_sites / process`。
  v1 配置会被自动替换为默认值。
- 下文中的"Claude"在多应用下泛指当前标签对应的应用。

## 1. 设计原则

| 原则 | 说明 |
|---|---|
| **不碰账号** | 检测全部由 ClaudeGate 自己的 HTTP 客户端发起，**不携带任何 Claude Cookie / Token**，不登录、不调用 Claude API、不打开 claude.ai 页面。对 Claude 域名只请求 Cloudflare 边缘的 `/cdn-cgi/trace`（这是 Cloudflare 节点直接返回的纯文本，不进入 Anthropic 应用服务器，也与账号无关），且该项可在设置中关闭。 |
| **检测与 Claude 同路** | `claude.ai/cdn-cgi/trace` 返回的 `ip=` 就是"Claude 看到的你的出口 IP"，与 ip.net.coffee 的做法一致，能如实反映分流规则下 Claude 走的线路。 |
| **硬性条件可配置** | 每个检测项都有 `启用 / 硬性(必须通过) / 阈值` 三个维度，硬性项失败 → 不允许启动；非硬性项失败 → 仅警告。 |
| **数据源可插拔** | IP 风险库默认用 ip.net.coffee，可切换到 ipapi.is / ip-api 等，避免单点失效。 |
| **不上报** | 不调用参考站点的 `/api/session` 之类的统计上报接口。 |
| **杀干净** | 关闭按进程树 + 名称/路径规则双重匹配，杀完再扫一遍确认无残留。 |

---

## 2. 技术选型

**Tauri 2（Rust 后端 + React/TypeScript 前端）**

- 产物小（Windows 安装包约 5–10 MB，对比 Electron 约 80 MB+）。
- Windows：输出 `ClaudeGate.exe`（便携）+ NSIS 安装包 `ClaudeGate_x.y.z_x64-setup.exe`。
- macOS：输出 `ClaudeGate.app` / `.dmg`（Intel + Apple Silicon 通用包）。
- 检测（HTTP / UDP STUN / DNS）、进程管理全部放在 Rust 侧，不受浏览器 CORS 限制，也不会和 Claude 共享浏览器指纹/Cookie。
- macOS 包必须在 Mac 上编译 → 使用 **GitHub Actions** 矩阵（windows-latest + macos-latest）自动打包，Windows 也可本地 `npm run tauri build`。

---

## 3. 总体结构

```
┌──────────────────────────── ClaudeGate (Tauri) ─────────────────────────────┐
│  前端 UI (React + TS)                                                        │
│   ├─ 首页：[开始检测] → 检测卡片网格（同截图样式） → [启动 Claude] [关闭 Claude] │
│   └─ 设置：基础 / 检测项与硬性条件 / 自定义站点 / 进程管理 / 网络与高级        │
│                         │  invoke() / event                                   │
│  ───────────────────────┼──────────────────────────────────────────────────── │
│  Rust 后端               ▼                                                    │
│   commands.rs   —— Tauri 命令入口                                             │
│   config.rs     —— 配置模型 + 读写 JSON（AppConfig 目录）                     │
│   detect/       —— 各检测器，并发执行，每项产出 CheckResult                   │
│     trace.rs        Cloudflare /cdn-cgi/trace（CF 出口、Claude 出口、自定义）  │
│     cn_ip.rs        国内出口 IP（ip138 / ip.cn / 自定义）                      │
│     iprisk.rs       IP 属性 + 信任分 + VPN/Proxy/Tor/滥用（可插拔数据源）      │
│     availability.rs claude.ai / anthropic.com 连通性与延迟                     │
│     status.rs       Claude 官方服务状态（status.claude.com 公共 JSON）         │
│     dns_leak.rs     DNS 泄露（随机子域 → 权威 DNS 记录解析器 IP）             │
│     stun.rs         UDP 出口（STUN Binding，等价 WebRTC 泄露检测）            │
│     device.rs       本机时区 / 语言与 IP 归属地一致性                         │
│     custom.rs       用户自定义站点：出口 IP（trace/回显）+ 入口 IP（DNS 解析） │
│   rules.rs      —— 根据配置把 CheckResult 判定为 Pass / Warn / Fail           │
│   launcher.rs   —— 启动 Claude（Win: MSIX AUMID / exe；Mac: open -b）         │
│   process.rs    —— 查找并整树杀掉 Claude 进程，二次扫描确认                   │
│   guard.rs      —— (可选) 运行期守护：定时复检，IP 变化/失败自动杀进程        │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. 检测项一览（对应截图）

| ID | 检测项 | 数据来源（默认） | 可设硬性条件示例 |
|---|---|---|---|
| `cn_ip` | 国内出口 IPv4 | `https://2026.ip138.com/`、`https://my.ip.cn/` | —（仅展示） |
| `cf_ip` | Cloudflare 出口 IP | `https://1.1.1.1/cdn-cgi/trace` | 必须非中国大陆 |
| `claude_ip` | Claude 出口 IP | `https://claude.ai/cdn-cgi/trace` | 必须获取成功；国家在允许列表 |
| `ip_consistent` | 出口一致性 | 比较 `claude_ip` / `cf_ip` / UDP 出口 | Claude 出口 = UDP 出口 |
| `trust_score` | 信任评分 | `https://ip.net.coffee/api/iprisk/{ip}` → `trust_score` | ≥ 阈值（默认 80） |
| `ip_type` | IP 属性 | 同上 `isResidential` / `is_datacenter` / `asn_kind` | 必须住宅 IP |
| `region` | 支持地区 | `countryCode` vs 受支持国家列表 | 不在受限地区（CN/HK/MO/RU/IR/KP…） |
| `vpn` / `proxy` / `tor` / `crawler` / `abuser` | 安全检测 | 同上 `is_vpn` 等 | 全部"未检测到" |
| `avail_claude` / `avail_anthropic` | 可用性 | `GET …/cdn-cgi/trace` 计时 | 可达且延迟 < N ms |
| `service_status` | 服务状态 | `https://status.claude.com/api/v2/status.json` | 非 major/critical |
| `dns_leak` | DNS 泄露 | 解析 `{token}-1.d.ip.net.coffee` → `GET /api/dns/result/{token}` | DNS 出口不在中国大陆 |
| `udp_leak` | UDP/WebRTC 泄露 | STUN `stun.cloudflare.com:3478`、`stun.l.google.com:19302` | UDP 出口 = Claude 出口 |
| `timezone` | 时区一致 | 本机 IANA 时区 vs IP 的 `timezone` | 一致（或同国家） |
| `language` | 语言 | 本机首选语言 | 首选语言非 zh-CN（可选） |
| `custom:*` | 自定义站点 | 用户配置 | 出口 IP 国家/与 Claude 出口一致等 |

> 截图里的 Canvas/WebGL 指纹是浏览器特征，对 Claude **桌面版**意义不大（桌面版是 Electron，指纹固定），默认只展示 GPU 信息不作判定。

### 4.1 自定义站点（"指定站点的出口与入口"）

每个站点一条配置：

```jsonc
{
  "name": "GitHub",
  "domain": "github.com",
  "egress": { "mode": "cf_trace" },              // cf_trace | ip_echo | none
  // ip_echo 模式: { "mode": "ip_echo", "url": "https://x/ip", "json_path": "ip" } 或 regex
  "ingress": { "resolve": true },                // 用系统 DNS 解析域名，得到入口 IP 及归属地
  "rules": { "required": false, "egress_same_as_claude": true, "egress_countries": ["US"] }
}
```

- **出口**：经由该站点看到的本机 IP（分流后走哪条线）。
- **入口**：本机把该域名解析成哪个 IP、归属哪里（可发现 DNS 污染 / 被解析到国内 CDN）。

---

## 5. 判定模型

```rust
enum Status { Pass, Warn, Fail, Skipped }

struct CheckResult {
    id: String, title: String,
    status: Status,          // 由 rules.rs 根据配置给出
    value: serde_json::Value,// 原始数据，前端渲染卡片
    message: String,         // 人类可读说明
    required: bool,          // 是否硬性
    duration_ms: u64,
}
```

- 所有检测并发执行（`tokio::join!`），整体超时可配。
- `can_launch = 所有 required=true 的检查都为 Pass`。
- 前端：`can_launch` 为真时显示"启动 Claude"大图标；否则灰掉并列出失败的硬性项。
- 可选"检测通过后自动启动"。

---

## 6. 启动与关闭

### 6.1 启动（launcher.rs）
| 平台 | 方式 |
|---|---|
| Windows (MSIX 商店版/官方新版) | `explorer.exe shell:AppsFolder\Claude_pzs8sxrjxfjjc!Claude`（AUMID 可配置，自动用 `Get-StartApps` 探测） |
| Windows (旧版 Squirrel) | `%LOCALAPPDATA%\AnthropicClaude\claude.exe` |
| macOS | `open -b com.anthropic.claudefordesktop`，或 `/Applications/Claude.app` |

启动前再做一次"快速复检"（只查 Claude 出口 IP 是否与刚才一致），防止检测后线路切换。

### 6.2 关闭（process.rs）
1. 按规则匹配进程（默认，可在设置中增删）：
   - Windows：`Claude.exe`（路径含 `WindowsApps\Claude_` 或 `AnthropicClaude`）、`claude.exe`（路径含 `Claude-Data` / `claude-code`）、`Squirrel.exe`（Claude 更新器）、以及它们的所有子进程（node/cmd/powershell/MCP server 等）。
   - macOS：`/Applications/Claude.app/**` 下所有进程（主进程、Helper、GPU、Renderer）、`ShipIt` 更新器、`claude-code` CLI 及子进程、Claude 拉起的 MCP server。
2. 先构建进程树，从叶子到根依次 kill（Windows `TerminateProcess` / `taskkill /T /F`；macOS `SIGTERM` → 等待 N 秒 → `SIGKILL`）。
3. 二次扫描，仍有残留则重试，最终给出"已清理 X 个进程 / 残留列表"。
4. 选项：
   - "保留独立运行的 Claude Code CLI"（只杀桌面版拉起的子树，不杀用户自己在终端开的 `claude`）。
   - 关闭 ClaudeGate 时是否顺带关闭 Claude。

### 6.3 运行期守护（可选）
Claude 运行期间每 N 秒检测 Claude 出口 IP；如 IP 变化、国家变化、或获取失败（VPN 断开），立即杀进程并弹窗提示——避免"VPN 掉线后 Claude 直连国内 IP"这种最常见的封号原因。

---

## 7. 配置模型（config.json，位于系统 AppConfig 目录）

```jsonc
{
  "version": 1,
  "general": {
    "language": "zh-CN",
    "auto_check_on_open": false,
    "auto_launch_when_pass": false,
    "kill_claude_on_exit": false,
    "guard": { "enabled": true, "interval_sec": 30, "kill_on_ip_change": true, "kill_on_fail": true }
  },
  "launcher": {
    "windows": { "mode": "auto", "aumid": "Claude_pzs8sxrjxfjjc!Claude", "exe_path": "" },
    "macos":   { "bundle_id": "com.anthropic.claudefordesktop", "app_path": "/Applications/Claude.app" },
    "recheck_before_launch": true
  },
  "network": {
    "timeout_ms": 8000,
    "proxy": "",                       // 留空=系统/直连；可指定 http/socks5 让检测与 Claude 同代理
    "user_agent": "Mozilla/5.0 ...",
    "risk_provider": "ipnetcoffee",    // ipnetcoffee | ipapi_is | ip_api
    "endpoints": {
      "cf_trace": "https://1.1.1.1/cdn-cgi/trace",
      "claude_trace": "https://claude.ai/cdn-cgi/trace",
      "anthropic_trace": "https://www.anthropic.com/cdn-cgi/trace",
      "cn_ip": ["https://2026.ip138.com/", "https://my.ip.cn/"],
      "status": "https://status.claude.com/api/v2/status.json",
      "stun": ["stun.cloudflare.com:3478", "stun.l.google.com:19302"],
      "dns_leak_base": "d.ip.net.coffee"
    }
  },
  "checks": {
    "claude_ip":      { "enabled": true, "required": true },
    "region":         { "enabled": true, "required": true, "allowed_countries": [], "blocked_countries": ["CN","HK","MO","RU","IR","KP","SY","CU","BY"] },
    "trust_score":    { "enabled": true, "required": true, "min": 80 },
    "ip_type":        { "enabled": true, "required": false, "require_residential": true },
    "vpn":            { "enabled": true, "required": false },
    "proxy":          { "enabled": true, "required": false },
    "tor":            { "enabled": true, "required": true },
    "abuser":         { "enabled": true, "required": true },
    "crawler":        { "enabled": true, "required": false },
    "avail_claude":   { "enabled": true, "required": true, "max_latency_ms": 3000 },
    "avail_anthropic":{ "enabled": true, "required": false, "max_latency_ms": 5000 },
    "service_status": { "enabled": true, "required": false, "fail_on": ["major","critical"] },
    "dns_leak":       { "enabled": true, "required": true, "blocked_countries": ["CN"] },
    "udp_leak":       { "enabled": true, "required": false, "must_equal_claude_ip": true },
    "ip_consistent":  { "enabled": true, "required": false, "cf_equals_claude": false },
    "timezone":       { "enabled": true, "required": false, "mode": "same_country" },
    "language":       { "enabled": false, "required": false, "forbid_primary": ["zh-CN"] }
  },
  "custom_sites": [],
  "process": {
    "windows_rules": [
      { "name": "Claude.exe",  "path_contains": ["WindowsApps\\Claude_", "AnthropicClaude"] },
      { "name": "claude.exe",  "path_contains": ["Claude-Data", "claude-code"] },
      { "name": "Squirrel.exe","path_contains": ["AnthropicClaude"] }
    ],
    "macos_rules": [
      { "path_contains": ["/Applications/Claude.app/"] },
      { "name": "ShipIt", "path_contains": ["com.anthropic"] },
      { "name": "claude",  "path_contains": ["claude-code"] }
    ],
    "kill_children": true,
    "keep_standalone_cli": true,
    "graceful_timeout_ms": 1500,
    "verify_rounds": 3
  }
}
```

设置页按上述分组呈现；所有项支持"恢复默认"、"导入/导出配置"。

---

## 8. 目录结构

```
ClaudeGate/
├─ docs/ARCHITECTURE.md
├─ package.json / vite.config.ts / tsconfig.json / index.html
├─ src/                      # 前端
│  ├─ main.tsx  App.tsx  api.ts  types.ts  styles.css
│  ├─ pages/Home.tsx         # 检测卡片 + 启动/关闭
│  └─ pages/Settings.tsx     # 配置表单
├─ src-tauri/
│  ├─ Cargo.toml  build.rs  tauri.conf.json  capabilities/
│  └─ src/
│     ├─ main.rs  lib.rs  commands.rs  config.rs  rules.rs
│     ├─ launcher.rs  process.rs  guard.rs  http.rs
│     └─ detect/{mod,trace,cn_ip,iprisk,availability,status,dns_leak,stun,device,custom}.rs
└─ .github/workflows/build.yml   # Windows + macOS 自动打包
```

---

## 9. 风险与注意事项

1. **第三方数据源**：信任分/DNS 泄露依赖 ip.net.coffee 的公开接口，接口变动会导致该项"无数据"。规则里"无数据"默认按 Warn 处理，可设为 Fail。
2. **请求 claude.ai 的唯一内容是 `/cdn-cgi/trace`**：无 Cookie、无登录态、与账号无关。如仍顾虑，可在设置中改用"仅 CF 出口"模式（前提是确认分流规则让 1.1.1.1 与 claude.ai 走同一线路）。
3. **检测线路 = Claude 线路？** ClaudeGate 与 Claude 都走系统代理/TUN 时才一致。若用的是"按进程分流"，需要把 ClaudeGate 也加入同一规则，或在设置中填写与 Claude 相同的代理地址。
4. **杀进程误伤**：默认规则用"名称 + 路径"双重匹配，不会误杀其他 Electron 应用；"保留独立 CLI"默认开启。
5. **macOS 权限**：杀进程/启动应用无需额外权限；未签名包首次打开需在"隐私与安全性"中允许，正式发布建议 Apple Developer ID 签名 + 公证。
