import { useState } from "react";
import Home from "./pages/Home";
import Settings from "./pages/Settings";

export default function App() {
  const [page, setPage] = useState<"home" | "settings">("home");
  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">ClaudeGate <span>Claude 安全启动器</span></div>
        <nav>
          <button className={page === "home" ? "tab active" : "tab"} onClick={() => setPage("home")}>检测与启动</button>
          <button className={page === "settings" ? "tab active" : "tab"} onClick={() => setPage("settings")}>系统配置</button>
        </nav>
      </header>
      <main>{page === "home" ? <Home /> : <Settings />}</main>
    </div>
  );
}
