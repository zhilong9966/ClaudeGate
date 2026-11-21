# PureGate 净启 给这些又爱又狠的IDE一个安全的使用环境

> 原名 PureGate
> <img width="1386" height="1050" alt="image" src="https://github.com/user-attachments/assets/ca96b203-c3da-4e8a-a05b-cfe688a7f91d" />


AI 桌面应用安全启动器（Windows / macOS）。支持 **Claude**、**Codex**、**Antigravity**，每个应用一个独立标签页。

- **启动前检测**：应用出口 IP、信任分、住宅/机房、VPN/代理/Tor/滥用、可用性、服务状态、DNS 泄露、UDP(WebRTC) 泄露、时区语言一致性、自定义站点出口/入口。
- **硬性条件可配置**：每个应用独立设置，全部硬性条件通过后才出现「启动」按钮。
- **一键关闭**：按进程树结束应用及其拉起的所有子进程，多轮扫描确认无残留。
- **运行期守护**：应用运行时定时复检出口 IP，VPN 掉线或线路切换立即结束该应用。
- **不碰账号**：检测使用独立 HTTP 客户端，无 Cookie、不登录、不调用任何 AI 服务的接口。
- **支持中心**：商务联系、企业级网络优化方案、企业授权申请。

架构详见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## 授权

**个人免费使用；企业、机构及任何商业用途需取得书面授权。** 详见 [LICENSE](LICENSE)。

商务合作 / 企业授权 / 企业级网络优化方案：**zhilongfeng66@gmail.com**

## 开发

需要 Node 20+、Rust stable。

```bash
npm install
npm run tauri dev       # 开发运行
npm run tauri build     # 打包（Windows: NSIS 安装包 + exe；macOS: .app/.dmg）
```

macOS 包需要在 Mac 上构建，或推送 `v*` tag 由 GitHub Actions 同时打包 Windows 与 macOS。

检测管线的只读联网测试（不会启动/结束任何进程）：

```bash
cd src-tauri && cargo test --lib -- --ignored --nocapture
```
