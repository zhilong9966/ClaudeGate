import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";

// 联系方式与方案内容集中在这里，改文案只需改这一处
export const CONTACT_EMAIL = "zhilongfeng66@gmail.com";
const REPO_URL = "https://github.com/zhilong9966/ClaudeGate";

const SOLUTIONS = [
  { title: "专属出口线路规划", desc: "按团队所在地与使用的 AI 服务，规划稳定、纯净的出口线路与节点，减少线路切换和 IP 漂移。" },
  { title: "分流与 DNS 防泄露配置", desc: "审计现有代理客户端的分流规则、TUN 与 DNS 设置，消除 DNS / UDP 泄露和多网卡并发解析问题。" },
  { title: "团队批量部署", desc: "为团队统一部署 ClaudeGate，集中下发检测规则与硬性条件，统一进程清理策略。" },
  { title: "企业授权与定制", desc: "企业内部使用授权、定制检测项、私有化检测服务端部署。" },
];

function mail(subject: string) {
  return openUrl(`mailto:${CONTACT_EMAIL}?subject=${encodeURIComponent(subject)}`);
}

export default function Support() {
  const [version, setVersion] = useState("");
  const [copied, setCopied] = useState(false);
  useEffect(() => { getVersion().then(setVersion).catch(() => {}); }, []);

  async function copy() {
    try {
      await navigator.clipboard.writeText(CONTACT_EMAIL);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* 剪贴板不可用时忽略 */ }
  }

  return (
    <div className="support">
      <div className="card wide">
        <div className="card-title">联系我们</div>
        <div className="contact">
          <div>
            <div className="sub">商务合作 · 企业授权 · 技术支持</div>
            <div className="email">{CONTACT_EMAIL}</div>
          </div>
          <div className="contact-actions">
            <button className="btn primary" onClick={() => mail("ClaudeGate 咨询")}>发送邮件</button>
            <button className="btn" onClick={copy}>{copied ? "已复制" : "复制邮箱"}</button>
          </div>
        </div>
      </div>

      <div className="card wide">
        <div className="card-title">企业级网络优化方案</div>
        <div className="solutions">
          {SOLUTIONS.map((s) => (
            <div className="solution" key={s.title}>
              <div className="solution-title">{s.title}</div>
              <div className="sub">{s.desc}</div>
            </div>
          ))}
        </div>
        <button className="btn primary" onClick={() => mail("企业级网络优化方案咨询")}>咨询方案</button>
      </div>

      <div className="grid two">
        <div className="card">
          <div className="card-title">软件授权</div>
          <div className="row"><span className="label">个人使用</span><span className="value"><span className="tag ok">免费</span></span></div>
          <div className="row"><span className="label">企业 / 商业使用</span><span className="value"><span className="tag warn">需授权</span></span></div>
          <p className="sub">企业、机构或任何商业用途须先取得书面授权，详见安装目录或仓库中的 LICENSE。</p>
          <button className="btn" onClick={() => mail("ClaudeGate 企业授权咨询")}>申请企业授权</button>
        </div>
        <div className="card">
          <div className="card-title">关于</div>
          <div className="row"><span className="label">版本</span><span className="value">{version || "—"}</span></div>
          <div className="row"><span className="label">项目主页</span><span className="value"><a href="#" onClick={(e) => { e.preventDefault(); openUrl(REPO_URL); }}>GitHub</a></span></div>
          <p className="sub">问题反馈与功能建议，欢迎邮件联系。</p>
        </div>
      </div>
    </div>
  );
}
