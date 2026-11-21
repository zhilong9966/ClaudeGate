import { useEffect, useState } from "react";
import { api } from "./api";
import logo from "../app-icon.svg";
import Home from "./pages/Home";
import Settings from "./pages/Settings";
import Support from "./pages/Support";
import type { AppProfile } from "./types";

const ICON: Record<string, string> = { claude: "✳", codex: "◎", antigravity: "▲" };

export default function App() {
  const [apps, setApps] = useState<AppProfile[]>([]);
  const [page, setPage] = useState<string>("");

  const reload = () =>
    api.getConfig().then((c) => {
      const list = c.apps.filter((a) => a.enabled);
      setApps(list);
      setPage((p) => p || list[0]?.id || "settings");
    });
  useEffect(() => { reload(); }, []);

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand"><img className="logo" src={logo} alt="" />PureGate <b>净启</b><span>AI 桌面应用安全启动器</span></div>
        <nav>
          {apps.map((a) => (
            <button key={a.id} className={page === a.id ? "tab active" : "tab"} onClick={() => setPage(a.id)}>
              <span className="tab-icon">{ICON[a.id] ?? "●"}</span>{a.name}
            </button>
          ))}
          <button className={page === "settings" ? "tab active" : "tab"} onClick={() => setPage("settings")}>系统配置</button>
          <button className={page === "support" ? "tab active" : "tab"} onClick={() => setPage("support")}>支持中心</button>
        </nav>
      </header>
      <main>
        {/* 每个应用标签常驻，切换时保留各自的检测结果 */}
        {apps.map((a) => (
          <div key={a.id} hidden={page !== a.id}>
            <Home appId={a.id} appName={a.name} active={page === a.id} />
          </div>
        ))}
        {page === "settings" && <Settings onSaved={reload} />}
        {page === "support" && <Support />}
      </main>
    </div>
  );
}
