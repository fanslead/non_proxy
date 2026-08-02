# NonProxy

> 本地优先、可验证的跨平台智能分流网关。

[![CI](https://github.com/fanslead/non_proxy/actions/workflows/ci.yml/badge.svg)](https://github.com/fanslead/non_proxy/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/fanslead/non_proxy?include_prereleases&label=release)](https://github.com/fanslead/non_proxy/releases)
[![macOS 15+](https://img.shields.io/badge/macOS-15%2B-111827?logo=apple)](docs/MACOS_SYSTEM_ACCEPTANCE.md)
[![Windows 10+](https://img.shields.io/badge/Windows-10%201903%2B-0078D4?logo=windows)](docs/WINDOWS_SYSTEM_ACCEPTANCE.md)

NonProxy 让用户通过选择应用或网站，明确决定对应流量是直接连接物理网络，还是经指定代理
出口访问。它统一处理应用身份、网站域名、DNS、TCP/UDP、策略发布、故障切换和路径证据，
不要求普通用户理解不同代理客户端的规则语法。

> [!IMPORTANT]
> NonProxy 目前处于 `0.0.x` 开发预览阶段。仓库测试、交叉编译或临时签名不等于正式安装包
> 已通过系统授权和真实 VPN 网络验收。请只从本仓库的
> [Releases](https://github.com/fanslead/non_proxy/releases) 页面下载发布产物，并核对对应版本的
> 签名与 SHA-256 清单。

## 为什么需要 NonProxy

普通 VPN 或代理客户端即使没有启用“全局模式”，仍可能因为系统路由、DNS、虚拟网卡或应用
自身行为接管不希望代理的流量。简单修改系统代理、PAC 或静态路由无法稳定覆盖所有软件、
IPv4/IPv6、TCP/UDP、QUIC、CDN 和网络切换场景。

NonProxy 将自身作为统一的流量决策点：

- 应用规则直接使用签名身份，不依赖容易冲突的进程名。
- 网站规则由浏览器标签页发起，不要求用户查找 API、登录或 CDN 域名。
- `DIRECT` 流量绑定可信物理路径，避免先进入第三方 VPN 再尝试绕出。
- `PROXY` 流量通过受控出口发送，失败时严格执行显式 `fail-closed` 或 `fail-open`。
- 配置、决策、实际路径和公网出口分别取证，不把“保存成功”误报成“已经直连”。

## 核心能力

| 能力 | 当前实现 |
|---|---|
| 应用直连 | macOS 签名应用身份；Windows Win32 Authenticode 与 MSIX/UWP 包身份 |
| 网站直连 | Safari/Chromium 扩展、标签页隔离、候选域名审核与幂等确认 |
| 统一数据面 | macOS Transparent/DNS Provider；Windows WFP TCP、DNS、UDP/QUIC |
| 代理出口 | SOCKS5、HTTP CONNECT、Shadowsocks TCP/UDP |
| 订阅管理 | HTTPS 公网安全获取、节点归属、自动刷新、失败退避和凭据回收 |
| 自动切换 | 有序线路组、固定目标健康探测、新连接切换和去敏审计 |
| 客户端协同 | Surge、Clash/Mihomo、sing-box 的显式登记、原生校验、原子写入与重载 |
| 策略发布 | 版本化快照、Provider ACK、上一有效配置恢复和五分钟紧急覆盖 |
| 可观测性 | 决策记录、物理/代理路径证据、签名出口回执和严格脱敏诊断包 |

当前内置 Shadowsocks 只接受六种 AEAD/AEAD-2022 方法，不接受 `none`、旧式流加密或
SIP003 plugin。VMess/VLESS、Trojan、Hysteria 2、TUIC、WireGuard、OpenVPN 和
OpenConnect 尚未作为内置协议开放。

## 工作原理

```mermaid
flowchart LR
    app["应用与浏览器"] --> capture["平台流量捕获"]
    dns["系统 DNS"] --> capture
    ui["桌面端与浏览器扩展"] --> control["认证控制面"]
    control --> store["SQLite 权威状态"]
    store --> snapshot["不可变策略快照"]
    snapshot --> capture
    capture --> decision{"策略决策"}
    decision -- "DIRECT" --> physical["物理网络"]
    decision -- "PROXY" --> gateway["NonProxy Gateway"]
    gateway --> outbound["代理出口或线路组"]
    decision --> evidence["决策与路径证据"]
```

控制面只管理配置；高权限平台组件只消费经过认证、版本化和完整校验的不可变快照。代理协议、
订阅解析和数据库操作不会进入 macOS Provider 或 Windows Driver。

## 平台状态

| 平台 | 源码与自动化门禁 | 正式安装与真实网络验收 |
|---|---|---|
| Apple Silicon / macOS 15+ | Avalonia App、System Extensions、LaunchAgent、Safari 扩展及跨语言 E2E 已接入 | 需要 Developer ID、Provisioning Profile、Notarization、系统授权和真实 VPN 矩阵 |
| Windows x64 / ARM64 | 共享桌面端、SCM Service、WFP Driver、安装/回滚工具及 Windows CI 已接入 | 需要企业代码签名、Microsoft 内核签名、WDK/SCM/UAC、Driver Verifier 和真实 VPN 矩阵 |

详细通过标准：

- [macOS 系统组件验收](docs/MACOS_SYSTEM_ACCEPTANCE.md)
- [Safari Web Extension 验收](docs/SAFARI_EXTENSION_ACCEPTANCE.md)
- [Windows 系统组件与真实网络路径验收](docs/WINDOWS_SYSTEM_ACCEPTANCE.md)

## 下载与安装

公开版本统一发布在 [GitHub Releases](https://github.com/fanslead/non_proxy/releases)。资产分为
两类：

- **开发预览版**：Release 标记为 Pre-release，资产名包含 `development`。它用于源码、UI、
  Bundle 和开发签名验证，并保留明确的系统权限限制。
- **正式版本**：完成平台正式签名与真实系统验收后发布。一个正式可安装版本必须同时提供：

- macOS：Developer ID 签名、公证并附票据的 App 安装介质。
- Windows：按 x64/ARM64 分包、固定发布者签名且包含 Microsoft 内核签名 Driver 的安装介质。
- 每个资产对应的 SHA-256 摘要和明确的版本说明。

开发预览版的具体信任步骤和限制见
[开发预览版发布说明](docs/DEVELOPMENT_RELEASE.md)。缺少正式门禁的 CI Artifact、临时签名
App 或测试签名 Driver 不作为面向普通用户的正式安装包发布。

## 从源码开始

### 环境要求

- macOS 开发机：Apple Silicon、macOS 15+、完整 Xcode。
- Windows Driver：Windows 11、Visual Studio 2026、固定 WDK/SDK NuGet 包、PowerShell 7.4+。
- 仓库固定版本：.NET 10、Rust、Node.js、pnpm、Buf、Protobuf Compiler 和 just。

macOS 可以把完整工具链安装在仓库自己的 `.tools/`，不会修改系统默认版本：

```bash
./scripts/bootstrap/install-local-tools.sh
source ./scripts/bootstrap/env.sh
./scripts/bootstrap/check-prerequisites.sh
```

### 完整验证

```bash
source ./scripts/bootstrap/env.sh
just check
```

`just check` 包含契约生成一致性、格式、lint、Rust/.NET/TypeScript/Swift 测试、桌面构建、
macOS Bundle 检查及控制面、Adapter、Native Messaging、Provider 跨语言冒烟。

常用的定向命令：

```bash
just contracts
just format-check
just lint
just test
just native-bridge-smoke
just gateway-bundle-smoke
```

修改 `.proto` 后还应运行：

```bash
just contracts-breaking
```

## 仓库结构

```text
apps/desktop/   Avalonia 共享桌面 UI 与 macOS/Windows 薄宿主
crates/         Rust 领域模型、策略、DNS、存储、协议和安全核心
services/       gatewayd、adapter-host 与出口探针
platform/       macOS Network/System Extension 与 Windows WFP
adapters/       Surge、Mihomo、sing-box 规则适配器
packages/       Safari/Chromium 浏览器扩展
proto/          跨进程契约的唯一来源
generated/      生成代码，禁止手工编辑
migrations/     只追加的 SQLite migration
scripts/        构建、验证、打包和系统生命周期工具
docs/           技术设计、ADR 与平台验收手册
```

## 安全与隐私

NonProxy 的核心不变量：

- 不进行 TLS MITM，不安装中间人根证书。
- 不读取网页正文、Cookie、表单、请求体或代理凭据。
- 密码、Token 和私钥只进入系统凭据库，不进入 SQLite、日志或诊断包。
- 不绕过 MDM、Always-On VPN 或组织强制安全策略。
- `PROXY` 失败不会未经用户授权静默退回直连。
- UI、浏览器扩展和远程订阅输入均不被高权限组件直接信任。
- 新配置必须先编译校验，再原子发布，并保留可验证的回滚点。

请不要在公开 Issue 中粘贴订阅 URL、代理密码、诊断原文、签名私钥或其他敏感材料。

## 文档

- [完整产品方案](NONPROXY_PRODUCT_SOLUTION.md)
- [技术实现文档](docs/TECHNICAL_IMPLEMENTATION.md)
- [开发预览版构建与签名](docs/DEVELOPMENT_RELEASE.md)
- [架构决策记录](docs/ADR)
- [AI 与工程协作规范](AGENTS.md)

## 参与开发

欢迎通过 [Issue](https://github.com/fanslead/non_proxy/issues) 报告可复现问题或讨论设计。提交代码前：

1. 阅读 [AGENTS.md](AGENTS.md) 和相关 ADR。
2. 保持模块边界，不把平台、高权限和 UI 逻辑耦合进同一文件。
3. 为行为变化补充失败、回滚和敏感信息边界测试。
4. 运行 `just check`，并在 PR 中区分仓库证据与真实设备验收。

## 许可证

仓库当前尚未声明统一的软件许可证。在根目录加入明确的 `LICENSE` 之前，源代码默认保留全部
权利；公开可见不等于获得复制、修改或再分发授权。
