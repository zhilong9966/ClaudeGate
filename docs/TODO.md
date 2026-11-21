# PureGate 净启 · 待办事项

> 已完成的功能见 git 历史和 [ARCHITECTURE.md](ARCHITECTURE.md)。这里只记录还没做的。

## 保护能力

- [ ] **Claude Code CLI 启动前检测**：在终端敲 `claude` 时先跑一遍该应用的硬性条件，通过才放行（做一个 `claude` 包装脚本 / PATH 前置），现在 CLI 只受运行期守护保护。
- [ ] **macOS 防火墙断网保护**：用 pf anchor（需 root）实现与 Windows 相同的「先断网再终止」和「未通过检测前禁止联网」，授权方式参考 Windows 的一次授权（launchd 守护进程 / SMJobBless）。
- [ ] **防火墙规则覆盖首次运行**：规则按精确路径生效，应用从未运行过（或升级后路径变了）时第一次无法提前阻断。可让提权同步程序自己枚举 `C:\Program Files\WindowsApps\Claude_*`、`OpenAI.Codex_*` 等目录补全路径。
- [ ] **卸载时清理**：NSIS 卸载钩子里删除计划任务 `\PureGate\PureGate Firewall Sync` 和所有 `PureGate-Block-*` 防火墙规则（现在需要先在设置里点「取消授权」）。
- [ ] **离线库补充普通代理 / 爬虫识别**：本地离线库目前识别不了这两项（检测里自动跳过），可找开源的代理 IP 列表补上。

## 基础设施

- [ ] **自建检测服务端（DMIT 洛杉矶服务器）**：权威 DNS 做 DNS 泄露检测，替换 ip.net.coffee。**需要一个域名**，把 `d.<域名>` 的 NS 指向服务器；顺便可提供 IP 回显接口。
- [ ] **GitHub Actions 解锁后出 Mac 包**：账号账单锁定解除后，打 `v0.3.0` 标签，CI 同时产出 Windows + macOS 安装包。
- [ ] **代码签名 + 自动更新**：Windows Authenticode 证书（避免 SmartScreen「未知发布者」）、Apple Developer ID 签名与公证；接入 Tauri updater（带签名校验）。

## 产品 / 商务

- [ ] **企业版配置锁定**：管理员设置硬性条件后加密码 / 下发签名配置，员工不能自行放宽。
- [ ] **商标检索**：正式商用前查询「PureGate」「净启」是否已被注册。
- [ ] **许可协议法务审阅**：[LICENSE](../LICENSE) 为起草稿，收费前请律师审阅。

## 待验证

- [ ] macOS 上 Codex / Antigravity 的 Bundle ID（`com.openai.codex`、`com.google.antigravity` 为推测值）。
- [ ] macOS 桌面快捷方式（替身）、开机自启（LaunchAgent）、网络变化轮询的实机测试。
- [ ] Windows 防火墙断网保护的实机测试：授权 → 模拟断 VPN → 确认先断网后结束、恢复后可正常启动。
