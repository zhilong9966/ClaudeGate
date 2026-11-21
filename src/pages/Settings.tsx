import { useEffect, useState } from "react";
import { api } from "../api";
import type { AppConfig, AppProfile, CustomSite, ProcInfo, ProcessRule } from "../types";
import { CHECK_TITLES, EGRESS_MODES, PARAM_TITLES } from "../types";

type Section = "general" | "network" | "launch" | "checks" | "egress" | "sites" | "process";
const GLOBAL: [Section, string][] = [["general", "基础与守护"], ["network", "网络与数据源"]];
const PER_APP: [Section, string][] = [
  ["launch", "启动方式"], ["checks", "检测项与硬性条件"], ["egress", "出口与探测"],
  ["sites", "自定义站点"], ["process", "进程管理"],
];

const csv = (v: unknown) => (Array.isArray(v) ? v.join(", ") : "");
const parseCsv = (s: string) => s.split(/[,，\s]+/).map((x) => x.trim()).filter(Boolean);

export default function Settings({ onSaved }: { onSaved: () => void }) {
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [section, setSection] = useState<Section>("general");
  const [appIdx, setAppIdx] = useState(0);
  const [msg, setMsg] = useState("");
  const [path, setPath] = useState("");

  useEffect(() => { api.getConfig().then(setCfg); api.configPath().then(setPath); }, []);
  if (!cfg) return <div>加载中…</div>;

  const up = (fn: (c: AppConfig) => void) => {
    const c = structuredClone(cfg);
    fn(c);
    setCfg(c);
  };
  const app = cfg.apps[appIdx];
  // 修改当前选中的应用
  const upApp = (fn: (a: AppProfile) => void) => up((c) => fn(c.apps[appIdx]));

  async function save() {
    try { await api.saveConfig(cfg!); setMsg("已保存"); onSaved(); } catch (e) { setMsg("保存失败：" + e); }
  }
  async function reset() {
    if (confirm("恢复全部默认配置？")) setCfg(await api.defaultConfig());
  }
  function exportCfg() {
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([JSON.stringify(cfg, null, 2)], { type: "application/json" }));
    a.download = "claudegate-config.json";
    a.click();
  }
  function importCfg(file?: File) {
    file?.text().then((t) => { try { setCfg(JSON.parse(t)); setMsg("已导入，记得保存"); } catch { setMsg("文件格式错误"); } });
  }
  const isAppSection = PER_APP.some(([k]) => k === section);

  return (
    <div className="settings">
      <aside>
        <div className="side-group">全局</div>
        {GLOBAL.map(([k, t]) => <button key={k} className={section === k ? "side active" : "side"} onClick={() => setSection(k)}>{t}</button>)}
        <div className="side-group">应用</div>
        <select className="app-select" value={appIdx} onChange={(e) => { setAppIdx(Number(e.target.value)); if (!isAppSection) setSection("launch"); }}>
          {cfg.apps.map((a, i) => <option key={a.id} value={i}>{a.name}{a.enabled ? "" : "（已停用）"}</option>)}
        </select>
        {PER_APP.map(([k, t]) => <button key={k} className={section === k ? "side active" : "side"} onClick={() => setSection(k)}>{t}</button>)}
        <div className="side-actions">
          <button className="btn primary" onClick={save}>保存</button>
          <button className="btn" onClick={reset}>恢复默认</button>
          <button className="btn" onClick={exportCfg}>导出配置</button>
          <label className="btn">导入配置<input type="file" accept=".json" hidden onChange={(e) => importCfg(e.target.files?.[0])} /></label>
          {msg && <div className="sub">{msg}</div>}
          <div className="sub small">{path}</div>
        </div>
      </aside>

      <section className="panel">
        {isAppSection && <div className="panel-app">当前应用：<b>{app.name}</b></div>}

        {section === "general" && (
          <>
            <h3>基础</h3>
            <Check label="打开 ClaudeGate 时自动检测当前标签" v={cfg.general.auto_check_on_open} on={(v) => up((c) => { c.general.auto_check_on_open = v; })} />
            <Check label="检测全部通过后自动启动应用" v={cfg.general.auto_launch_when_pass} on={(v) => up((c) => { c.general.auto_launch_when_pass = v; })} />
            <Check label="退出 ClaudeGate 时同时关闭所有已启用的应用" v={cfg.general.kill_apps_on_exit} on={(v) => up((c) => { c.general.kill_apps_on_exit = v; })} />
            <h3>运行期守护</h3>
            <p className="sub">应用运行期间定时检测出口 IP，VPN 掉线 / 线路切换时立即结束该应用，防止直连裸奔。每个应用独立守护。</p>
            <Check label="启用守护" v={cfg.general.guard.enabled} on={(v) => up((c) => { c.general.guard.enabled = v; })} />
            <Num label="检测间隔（秒）" v={cfg.general.guard.interval_sec} on={(v) => up((c) => { c.general.guard.interval_sec = v; })} />
            <Check label="出口 IP 变化时结束应用" v={cfg.general.guard.kill_on_ip_change} on={(v) => up((c) => { c.general.guard.kill_on_ip_change = v; })} />
            <Check label="出口 IP 获取失败时结束应用" v={cfg.general.guard.kill_on_fail} on={(v) => up((c) => { c.general.guard.kill_on_fail = v; })} />
          </>
        )}

        {section === "network" && (
          <>
            <h3>网络</h3>
            <Num label="超时（毫秒）" v={cfg.network.timeout_ms} on={(v) => up((c) => { c.network.timeout_ms = v; })} />
            <Text label="检测使用的代理（留空=系统/直连，如 socks5://127.0.0.1:7891）" v={cfg.network.proxy} on={(v) => up((c) => { c.network.proxy = v; })} />
            <Text label="User-Agent" v={cfg.network.user_agent} on={(v) => up((c) => { c.network.user_agent = v; })} />
            <Sel label="IP 风险数据源" v={cfg.network.risk_provider} opts={["ipnetcoffee", "ipapi_is"]} on={(v) => up((c) => { c.network.risk_provider = v; })} />
            <h3>公共检测端点（所有应用共用）</h3>
            {Object.entries(cfg.network.endpoints).map(([k, v]) => (
              <label className="field" key={k}>
                <span>{k}</span>
                {Array.isArray(v)
                  ? <input defaultValue={csv(v)} onBlur={(e) => up((c) => { c.network.endpoints[k] = parseCsv(e.target.value); })} />
                  : <input value={v} onChange={(e) => up((c) => { c.network.endpoints[k] = e.target.value; })} />}
              </label>
            ))}
          </>
        )}

        {section === "launch" && (
          <>
            <h3>启动方式</h3>
            <Check label="在顶部显示该应用的标签" v={app.enabled} on={(v) => upApp((a) => { a.enabled = v; })} />
            <Text label="显示名称" v={app.name} on={(v) => upApp((a) => { a.name = v; })} />
            <Sel label="Windows 启动方式" v={app.launcher.windows_mode} opts={["auto", "aumid", "exe"]} on={(v) => upApp((a) => { a.launcher.windows_mode = v; })} />
            <Text label="Windows AppUserModelID（MSIX / 开始菜单应用）" v={app.launcher.windows_aumid} on={(v) => upApp((a) => { a.launcher.windows_aumid = v; })} />
            <Text label="Windows exe 路径（mode=exe 或 AUMID 为空时使用）" v={app.launcher.windows_exe_path} on={(v) => upApp((a) => { a.launcher.windows_exe_path = v; })} />
            <Text label="macOS Bundle ID" v={app.launcher.macos_bundle_id} on={(v) => upApp((a) => { a.launcher.macos_bundle_id = v; })} />
            <Text label="macOS App 路径" v={app.launcher.macos_app_path} on={(v) => upApp((a) => { a.launcher.macos_app_path = v; })} />
            <Check label="启动前快速复检出口 IP" v={app.launcher.recheck_before_launch} on={(v) => upApp((a) => { a.launcher.recheck_before_launch = v; })} />
          </>
        )}

        {section === "checks" && (
          <>
            <h3>检测项与硬性条件</h3>
            <p className="sub">「硬性」= 必须通过才显示启动按钮；非硬性项失败只做警告。</p>
            <table className="checks">
              <thead><tr><th>检测项</th><th>启用</th><th>硬性</th><th>参数</th></tr></thead>
              <tbody>
                {Object.entries(app.checks).map(([id, r]) => (
                  <tr key={app.id + id}>
                    <td>{CHECK_TITLES[id] ?? id}</td>
                    <td><input type="checkbox" checked={r.enabled} onChange={(e) => upApp((a) => { a.checks[id].enabled = e.target.checked; })} /></td>
                    <td><input type="checkbox" checked={r.required} onChange={(e) => upApp((a) => { a.checks[id].required = e.target.checked; })} /></td>
                    <td className="params">
                      {Object.entries(r).filter(([k]) => k !== "enabled" && k !== "required").map(([k, v]) => (
                        <label key={k} className="param">
                          <span>{PARAM_TITLES[k] ?? k}</span>
                          {typeof v === "boolean" ? (
                            <input type="checkbox" checked={v} onChange={(e) => upApp((a) => { a.checks[id][k] = e.target.checked; })} />
                          ) : typeof v === "number" ? (
                            <input type="number" value={v} onChange={(e) => upApp((a) => { a.checks[id][k] = Number(e.target.value); })} />
                          ) : Array.isArray(v) ? (
                            <input key={app.id} defaultValue={csv(v)} onBlur={(e) => upApp((a) => { a.checks[id][k] = parseCsv(e.target.value); })} />
                          ) : (
                            <input value={String(v)} onChange={(e) => upApp((a) => { a.checks[id][k] = e.target.value; })} />
                          )}
                        </label>
                      ))}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        )}

        {section === "egress" && (
          <>
            <h3>出口 IP 获取方式</h3>
            <p className="sub">决定「{app.name} 服务端看到的你的 IP」怎么取。只请求无需登录的地址，不带任何账号 Cookie。</p>
            <Sel label="模式" v={app.egress.mode} opts={Object.keys(EGRESS_MODES)} labels={EGRESS_MODES} on={(v) => upApp((a) => { a.egress.mode = v; })} />
            <Text label="地址" v={app.egress.url} on={(v) => upApp((a) => { a.egress.url = v; })} />
            {app.egress.mode === "ip_echo" && <Text label="JSON 字段（空=正则提取 IPv4）" v={app.egress.json_field} on={(v) => upApp((a) => { a.egress.json_field = v; })} />}
            <h3>可用性探测</h3>
            <p className="sub">「必需」= 该地址不可达或超时会阻止启动（需同时开启检测项「可用性」的硬性）。</p>
            {app.probes.map((p, i) => (
              <div className="site site-grid" key={app.id + i}>
                <Text label="名称" v={p.name} on={(v) => upApp((a) => { a.probes[i].name = v; })} />
                <Text label="URL" v={p.url} on={(v) => upApp((a) => { a.probes[i].url = v; })} />
                <Num label="最大延迟（ms）" v={p.max_latency_ms} on={(v) => upApp((a) => { a.probes[i].max_latency_ms = v; })} />
                <Check label="必需" v={p.required} on={(v) => upApp((a) => { a.probes[i].required = v; })} />
                <button className="btn danger small" onClick={() => upApp((a) => { a.probes.splice(i, 1); })}>删除</button>
              </div>
            ))}
            <button className="btn" onClick={() => upApp((a) => { a.probes.push({ name: "", url: "", required: false, max_latency_ms: 5000 }); })}>+ 添加探测地址</button>
            <h3>服务状态页</h3>
            <Text label="Statuspage JSON 地址（留空=不检测）" v={app.status_url} on={(v) => upApp((a) => { a.status_url = v; })} />
          </>
        )}

        {section === "sites" && (
          <>
            <h3>自定义站点（出口 / 入口检测）</h3>
            <p className="sub">出口 = 访问该站点时对方看到的你的 IP；入口 = 该域名被本机 DNS 解析到的 IP。</p>
            {app.custom_sites.map((s, i) => (
              <div className="site" key={app.id + i}>
                <SiteEditor s={s} appName={app.name} on={(ns) => upApp((a) => { a.custom_sites[i] = ns; })} />
                <button className="btn danger small" onClick={() => upApp((a) => { a.custom_sites.splice(i, 1); })}>删除</button>
              </div>
            ))}
            <button className="btn" onClick={() => upApp((a) => {
              a.custom_sites.push({ name: "", domain: "", egress_mode: "cf_trace", egress_url: "", egress_json_field: "", resolve_ingress: true, required: false, egress_same_as_claude: false, egress_countries: [] });
            })}>+ 添加站点</button>
          </>
        )}

        {section === "process" && <ProcessTab key={app.id} app={app} upApp={upApp} />}
      </section>
    </div>
  );
}

function ProcessTab({ app, upApp }: { app: AppProfile; upApp: (fn: (a: AppProfile) => void) => void }) {
  const [procs, setProcs] = useState<ProcInfo[] | null>(null);
  const isMac = navigator.userAgent.includes("Mac");
  const key = isMac ? "macos_rules" : "windows_rules";
  const rules: ProcessRule[] = app.process[key];
  return (
    <>
      <h3>进程匹配规则（{isMac ? "macOS" : "Windows"}）</h3>
      <p className="sub">进程名、路径、命令行三项同时满足即命中（留空的项不限制）；一项里填多个时任一包含即可。</p>
      {rules.map((r, i) => (
        <div className="site" key={i}>
          <Text label="进程名（空=不限）" v={r.name} on={(v) => upApp((a) => { a.process[key][i].name = v; })} />
          <label className="field"><span>路径包含（逗号分隔）</span>
            <input defaultValue={csv(r.path_contains)} onBlur={(e) => upApp((a) => { a.process[key][i].path_contains = parseCsv(e.target.value); })} /></label>
          <label className="field"><span>命令行包含（逗号分隔，用于 npm 安装的 CLI 等）</span>
            <input defaultValue={csv(r.cmd_contains)} onBlur={(e) => upApp((a) => { a.process[key][i].cmd_contains = parseCsv(e.target.value); })} /></label>
          <button className="btn danger small" onClick={() => upApp((a) => { a.process[key].splice(i, 1); })}>删除</button>
        </div>
      ))}
      <button className="btn" onClick={() => upApp((a) => { a.process[key].push({ name: "", path_contains: [], cmd_contains: [] }); })}>+ 添加规则</button>
      <h3>结束策略</h3>
      <Check label="连同子进程一起结束（MCP server、终端、language server 等）" v={app.process.kill_children} on={(v) => upApp((a) => { a.process.kill_children = v; })} />
      <Check label="保留独立运行的 CLI（不勾选 = 关闭时连同终端里单独运行的 CLI 一起结束）" v={app.process.keep_standalone_cli} on={(v) => upApp((a) => { a.process.keep_standalone_cli = v; })} />
      <label className="field"><span>CLI 路径特征（逗号分隔）</span>
        <input defaultValue={csv(app.process.cli_markers)} onBlur={(e) => upApp((a) => { a.process.cli_markers = parseCsv(e.target.value); })} /></label>
      <label className="field"><span>桌面版路径特征（逗号分隔）</span>
        <input defaultValue={csv(app.process.desktop_markers)} onBlur={(e) => upApp((a) => { a.process.desktop_markers = parseCsv(e.target.value); })} /></label>
      <Num label="优雅退出等待（毫秒，仅 macOS）" v={app.process.graceful_timeout_ms} on={(v) => upApp((a) => { a.process.graceful_timeout_ms = v; })} />
      <Num label="扫描确认轮数" v={app.process.verify_rounds} on={(v) => upApp((a) => { a.process.verify_rounds = v; })} />
      <h3>预览</h3>
      <p className="sub">按「已保存」的规则列出当前会被结束的进程（只列出，不会结束）。</p>
      <button className="btn" onClick={() => api.listProcesses(app.id).then(setProcs)}>列出匹配进程</button>
      {procs && (procs.length === 0 ? <p className="sub">当前没有匹配的进程。</p> : (
        <table className="checks">
          <thead><tr><th>PID</th><th>名称</th><th>路径</th></tr></thead>
          <tbody>{procs.map((p) => <tr key={p.pid}><td>{p.pid}</td><td>{p.name}</td><td className="small">{p.path}</td></tr>)}</tbody>
        </table>
      ))}
    </>
  );
}

function SiteEditor({ s, appName, on }: { s: CustomSite; appName: string; on: (s: CustomSite) => void }) {
  const set = <K extends keyof CustomSite>(k: K, v: CustomSite[K]) => on({ ...s, [k]: v });
  return (
    <div className="site-grid">
      <Text label="名称" v={s.name} on={(v) => set("name", v)} />
      <Text label="域名（如 github.com）" v={s.domain} on={(v) => set("domain", v)} />
      <Sel label="出口检测方式" v={s.egress_mode} opts={["cf_trace", "ip_echo", "none"]} on={(v) => set("egress_mode", v)} />
      {s.egress_mode === "ip_echo" && <>
        <Text label="IP 回显 URL" v={s.egress_url} on={(v) => set("egress_url", v)} />
        <Text label="JSON 字段（空=正则提取）" v={s.egress_json_field} on={(v) => set("egress_json_field", v)} />
      </>}
      <Check label="解析入口 IP" v={s.resolve_ingress} on={(v) => set("resolve_ingress", v)} />
      <Check label="硬性条件" v={s.required} on={(v) => set("required", v)} />
      <Check label={`出口须与 ${appName} 出口一致`} v={s.egress_same_as_claude} on={(v) => set("egress_same_as_claude", v)} />
      <label className="field"><span>出口允许国家（逗号分隔，空=不限）</span>
        <input defaultValue={csv(s.egress_countries)} onBlur={(e) => set("egress_countries", parseCsv(e.target.value))} /></label>
    </div>
  );
}

function Check({ label, v, on }: { label: string; v: boolean; on: (v: boolean) => void }) {
  return <label className="field check"><input type="checkbox" checked={v} onChange={(e) => on(e.target.checked)} /><span>{label}</span></label>;
}
function Num({ label, v, on }: { label: string; v: number; on: (v: number) => void }) {
  return <label className="field"><span>{label}</span><input type="number" value={v} onChange={(e) => on(Number(e.target.value))} /></label>;
}
function Text({ label, v, on }: { label: string; v: string; on: (v: string) => void }) {
  return <label className="field"><span>{label}</span><input value={v} onChange={(e) => on(e.target.value)} /></label>;
}
function Sel({ label, v, opts, labels, on }: { label: string; v: string; opts: string[]; labels?: Record<string, string>; on: (v: string) => void }) {
  return (
    <label className="field"><span>{label}</span>
      <select value={v} onChange={(e) => on(e.target.value)}>{opts.map((o) => <option key={o} value={o}>{labels?.[o] ?? o}</option>)}</select>
    </label>
  );
}
