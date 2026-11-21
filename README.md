# ClaudeGate

Claude 桌面版安全启动器（Windows / macOS）。

- **启动前检测**：Claude 出口 IP、信任分、住宅/机房、VPN/代理/Tor/滥用、可用性、服务状态、DNS 泄露、UDP(WebRTC) 泄露、时区语言一致性、自定义站点出口/入口。
- **硬性条件可配置**：全部硬性条件通过后才出现「启动 Claude」按钮。
- **一键关闭**：按进程树结束 Claude 桌面版及其拉起的所有子进程，多轮扫描确认无残留。
- **运行期守护**：Claude 运行时定时复检出口 IP，VPN 掉线或线路切换立即结束 Claude。
- **不碰账号**：检测使用独立 HTTP 客户端，无 Cookie、不登录、不调用 Claude API；对 Claude 域名只请求 Cloudflare 边缘的 `/cdn-cgi/trace`。

架构详见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

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
