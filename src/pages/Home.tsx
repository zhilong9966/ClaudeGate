import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type { CheckResult, Report, Status } from "../types";

const TAG: Record<Status, string> = { pass: "tag ok", warn: "tag warn", fail: "tag bad" };
const TAG_TEXT: Record<Status, string> = { pass: "通过", warn: "警告", fail: "未通过" };
const ICON: Record<string, string> = { claude: "✳", codex: "◎", antigravity: "▲" };

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="row">
      <span className="label">{label}</span>
      <span className="value">{children}</span>
    </div>
  );
}

function Flag({ v }: { v?: boolean }) {
  if (v === undefined || v === null) return <span className="tag muted">无数据</span>;
  return v ? <span className="tag bad">检测到</span> : <span className="tag ok">未检测到</span>;
}

export default function Home({ appId, appName, active }: { appId: string; appName: string; active: boolean }) {
  const [report, setReport] = useState<Report | null>(null);
  const [checking, setChecking] = useState(false);
  const [msg, setMsg] = useState("");
  const [guard, setGuard] = useState("");

  useEffect(() => {
    const un = listen<{ app_id: string; kind: string; message: string }>("guard", (e) => {
      if (e.payload.app_id !== appId) return;
      if (e.payload.kind === "killed") {
        setGuard("⚠ 守护已触发：" + e.payload.message);
        setReport(null);
      } else if (e.payload.kind === "ok") {
        setGuard(`守护中 · ${appName} 出口 ${e.payload.message} 正常`);
      } else setGuard(e.payload.message);
    });
    return () => { un.then((f) => f()); };
  }, [appId, appName]);

  // 打开应用时自动检测：只对当前显示的标签执行一次
  useEffect(() => {
    if (!active || report || checking) return;
    api.getConfig().then((c) => { if (c.general.auto_check_on_open) start(); });
  }, [active]);

  async function start() {
    setChecking(true);
    setMsg("");
    setReport(null);
    try {
      const r = await api.runChecks(appId);
      setReport(r);
      const cfg = await api.getConfig();
      if (r.can_launch && cfg.general.auto_launch_when_pass) await launch();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setChecking(false);
    }
  }

  async function launch() {
    try { setMsg(await api.launch(appId)); } catch (e) { setMsg("启动失败：" + e); }
  }

  async function kill() {
    try {
      const r = await api.kill(appId);
      setMsg(`已结束 ${r.killed.length} 个进程` + (r.remaining.length ? `，残留 ${r.remaining.length} 个：${r.remaining.map((p) => p.name + "#" + p.pid).join(", ")}` : "，无残留"));
      setGuard("");
    } catch (e) { setMsg("关闭失败：" + e); }
  }

  const f = report?.facts;
  const failed = report?.checks.filter((c) => c.required && c.status !== "pass") ?? [];
  const byId = (id: string): CheckResult | undefined => report?.checks.find((c) => c.id === id);

  return (
    <div className="home">
      <section className="actions">
        <button className="btn primary" disabled={checking} onClick={start}>
          {checking ? "检测中…" : report ? "重新检测" : "开始检测"}
        </button>
        {report?.can_launch && (
          <button className={`btn launch launch-${appId}`} onClick={launch} title="所有硬性条件已通过">
            <span className="launch-icon">{ICON[appId] ?? "●"}</span> 启动 {appName}
          </button>
        )}
        <button className="btn danger" onClick={kill}>关闭 {appName}（结束全部进程）</button>
      </section>
      {msg && <div className="notice">{msg}</div>}
      {guard && <div className="notice guard">{guard}</div>}
      {report && !report.can_launch && (
        <div className="notice bad">未通过的硬性条件：{failed.map((c) => `${c.title}（${c.message}）`).join("；")}</div>
      )}
      {!report && !checking && <div className="empty">点击「开始检测」检查 {appName} 的网络环境，硬性条件全部通过后才会出现启动按钮。</div>}

      {f && (
        <>
          <div className="grid">
            <div className="card"><div className="card-title">国内出口 IPv4</div>
              <div className="big-ip">{f.cn_ip?.ip ?? "—"}</div><div className="sub">{f.cn_ip?.raw}</div></div>
            <div className="card"><div className="card-title">Cloudflare 出口 IP</div>
              <div className="big-ip">{f.cf?.ip ?? "—"}</div><div className="sub">{f.cf?.loc} · {f.cf?.colo}</div></div>
            <div className="card"><div className="card-title">{appName} 出口 IP</div>
              <div className="big-ip">{f.egress?.ip ?? "—"}</div>
              <div className="sub">
                {f.egress ? `${f.risk?.country ?? f.egress.loc} ${f.risk?.as_org ?? ""}` : f.egress_error}
                {f.egress?.vendor_subnet && <><br />厂商侧看到的网段：{f.egress.vendor_subnet}</>}
              </div></div>

            <div className="card"><div className="card-title">信任评分 (Trust Score)</div>
              <div className="score" style={{ color: scoreColor(f.risk?.trust_score) }}>{f.risk?.trust_score ?? "—"}</div>
              <div className="gauge"><div className="pointer" style={{ left: `${f.risk?.trust_score ?? 0}%` }} /></div>
              <Row label={`${appName} 支持地区`}><StatusTag c={byId("region")} /></Row></div>
            <div className="card"><div className="card-title">{appName} 出口 IP 属性</div>
              <Row label="地区">{f.risk?.country || "—"}</Row>
              <Row label="IP 属性">{f.risk?.is_residential ? <span className="tag ok">家庭住宅 IP</span> : <span className="tag warn">机房/其他</span>} ({f.risk?.company_type})</Row>
              <Row label="ASN">AS{f.risk?.asn ?? "—"}</Row>
              <Row label="运营商">{f.risk?.as_org || "—"}</Row>
              <Row label="数据源">{f.risk?.provider || "—"}</Row></div>
            <div className="card"><div className="card-title">{appName} 出口 IP 安全检测</div>
              <Row label="VPN"><Flag v={f.risk?.is_vpn} /></Row>
              <Row label="代理 (Proxy)"><Flag v={f.risk?.is_proxy} /></Row>
              <Row label="Tor"><Flag v={f.risk?.is_tor} /></Row>
              <Row label="机器人 (Crawler)"><Flag v={f.risk?.is_crawler} /></Row>
              <Row label="滥用记录"><Flag v={f.risk?.is_abuser} /></Row></div>

            <div className="card"><div className="card-title">{appName} 可用性检测</div>
              {f.avail.map((a) => (
                <Row key={a.name} label={a.name}>{a.ok ? <><span className={a.latency_ms > 1000 ? "tag warn" : "tag ok"}>{a.latency_ms > 1000 ? "较慢" : "正常"}</span> {a.latency_ms}ms</> : <span className="tag bad">不可访问</span>}</Row>
              ))}
              <Row label={`${appName} 服务状态`}>{f.status ? <span className={f.status.indicator === "none" ? "tag ok" : "tag warn"}>{f.status.description}</span> : <span className="tag muted">无状态页</span>}</Row></div>
            <div className="card"><div className="card-title">DNS 泄露检测</div>
              <Row label="状态"><StatusTag c={byId("dns_leak")} /></Row>
              {f.dns?.servers.map((s) => <Row key={s.ip} label="DNS 出口">{s.ip} ({s.country_code})</Row>)}</div>
            <div className="card"><div className="card-title">WebRTC UDP 泄露检测</div>
              <Row label="状态"><StatusTag c={byId("udp_leak")} /></Row>
              <Row label="UDP 出口">{f.udp?.ip ?? "—"}</Row>
              <Row label="归属地">{f.udp?.country_code ?? "—"}</Row></div>
          </div>

          <div className="card wide"><div className="card-title">本机环境</div>
            <Row label="时区"><StatusTag c={byId("timezone")} /> {f.device.timezone}</Row>
            <Row label="语言">{f.device.locales.join(", ")}</Row>
            <Row label="系统">{f.device.os} / {f.device.arch}</Row></div>

          {f.custom.length > 0 && (
            <div className="card wide"><div className="card-title">自定义站点 出口 / 入口</div>
              {f.custom.map((s) => (
                <Row key={s.domain} label={s.name || s.domain}>
                  出口 {s.egress_ip || s.egress_error || "—"} {s.egress_country} · 入口 {s.ingress_ips.join(", ") || s.ingress_error || "—"} {s.ingress_country}
                </Row>
              ))}</div>
          )}

          <div className="card wide"><div className="card-title">判定明细</div>
            {report!.checks.map((c) => (
              <Row key={c.id} label={(c.required ? "★ " : "") + c.title}><span className={TAG[c.status]}>{TAG_TEXT[c.status]}</span> {c.message}</Row>
            ))}
            <div className="sub">★ = 硬性条件（必须通过才能启动），可在「系统配置 → {appName}」中调整</div></div>
        </>
      )}
    </div>
  );
}

function StatusTag({ c }: { c?: CheckResult }) {
  if (!c) return <span className="tag muted">未启用</span>;
  return <span className={TAG[c.status]}>{TAG_TEXT[c.status]}</span>;
}

function scoreColor(s?: number) {
  if (s === undefined) return "#9ca3af";
  return s >= 95 ? "#22c55e" : s >= 80 ? "#16a34a" : s >= 50 ? "#84cc16" : s >= 25 ? "#eab308" : "#ef4444";
}
