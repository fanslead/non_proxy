# NonProxy

NonProxy 是一个本地优先的跨平台智能分流网关。用户选择应用或网站后，系统把匹配流量明确路由为 `DIRECT` 或 `PROXY`，并保留决策、路径和出口证据。

当前状态：按 `docs/TECHNICAL_IMPLEMENTATION.md` 实施中。仓库内的构建通过不代表 macOS System Extension、Windows WFP 或真实网络路径已经验收。

共享基础已经覆盖版本化契约、Rust 策略/编译器/SQLite 权威存储、认证本地控制面、Avalonia 工作区、SOCKS5/HTTP CONNECT 出口、系统凭据库和有背压的 NPF1 TCP/UDP 数据通道。浏览器学习链路包含 Safari/Chromium 正确入口、标签页隔离、临时站点权限、候选审核和幂等规则确认；macOS 已具备真实 Transparent/DNS Provider、物理 DIRECT、代理出口、System Extension Bundle、LaunchAgent 生命周期与打包冒烟。临时签名、交叉构建和夹具回显只属于仓库证据，正式签名、系统授权与真实 VPN 共存仍需独立验收。

Windows 已接入 SCM Service、受限命名管道、共享 UI、最小 WFP ALE Connect Redirect Driver、动态 BFE session、版本化 IOCTL/context ABI、redirect records 和用户态 TCP DIRECT/PROXY/BLOCK。DIRECT TCP/DNS 会实时选择可信物理接口并设置 `IP_UNICAST_IF`/`IPV6_UNICAST_IF`；没有物理路径时明确失败。域名身份运行时会在确认 `198.18.0.0/15` 无路由冲突后分配可恢复的 IPv4/安装级 ULA 合成地址，由 WFP TCP 代理反查域名并重新物理解域或保留给远端代理。

Windows DNS 不修改网卡设置：动态 WFP filter 先只把远端 TCP/UDP 53 重定向到随机 loopback listener，系统 resolver 探针通过且活动策略就绪后才启用普通 TCP；探针失效则退回 DNS-only。远端 53 之外的 UDP/QUIC 使用同一最小 Driver 的 ALE flow 身份关联、DATAGRAM_DATA 有界搬运和入站重注入，Service 继续执行 App/域名策略、物理 DIRECT 或 SOCKS5 UDP。

Windows 发行层已经加入 Primitive Driver INF、固定发布者 + 已签名清单绑定、复制后复验、版本化 Service/Driver 安装、失败回滚、默认保留数据的卸载、Driver Verifier 安全门与生命周期证据清单。当前 Rust 单测和 Windows 交叉门禁覆盖用户态与 ABI；WDK 实机结果、Hardware Dev Center 生产签名、SCM/UAC、Driver Verifier 与真实 VPN 路径尚未验收，因此 Windows UI 仍不会把系统组件表述为可用。

运行概览会按“后台服务 → 透明代理 → DNS 分流 → 网络接管”显示真实分段状态。等待授权时可通过原生桥直接打开 macOS“登录项与扩展”，允许后重新检查；部分安装可执行修复，卸载需要二次确认。诊断页复用同一组分段证据并显示稳定错误码。

正式签名 macOS 包的只读查询、安装、升级、卸载和完整生命周期验收使用 [macOS 系统组件验收手册](docs/MACOS_SYSTEM_ACCEPTANCE.md)。验收命令拒绝临时签名、非 `/Applications` 包和未经显式确认的系统变更，并输出带 SHA-256 清单的独立证据目录。

Safari 扩展的正式登记、启用、普通/无痕窗口与多标签页验收使用 [Safari Web Extension 正式验收](docs/SAFARI_EXTENSION_ACCEPTANCE.md)。

Windows 的构建签名、安装/修复/升级/卸载、Driver Verifier 和真实 VPN
TCP/DNS/UDP/QUIC 验收使用
[Windows 系统组件与真实网络路径验收](docs/WINDOWS_SYSTEM_ACCEPTANCE.md)。

## 架构

- `apps/desktop/`：唯一的 Avalonia 跨平台桌面 UI。
- `crates/`：Rust 领域模型、策略、DNS、存储和安全核心。
- `services/`：`gatewayd`、适配器宿主和测试探针。
- `platform/`：macOS Network/System Extension 与 Windows WFP。
- `packages/`：浏览器扩展和 TypeScript 共享包。
- `proto/`：跨进程契约唯一来源。
- `generated/`：生成代码，禁止手工编辑。

完整边界和交付顺序见：

- [产品方案](NONPROXY_PRODUCT_SOLUTION.md)
- [技术实现](docs/TECHNICAL_IMPLEMENTATION.md)
- [桌面 UI ADR](docs/ADR/0001-use-avalonia-for-cross-platform-desktop-ui.md)
- [macOS Provider 策略运行时 ADR](docs/ADR/0002-use-native-provider-policy-runtime.md)
- [macOS 最低版本 ADR](docs/ADR/0003-set-macos-15-minimum.md)
- [Windows 最小 WFP Callout ADR](docs/ADR/0004-use-minimal-wfp-connect-redirect-callout.md)
- [Windows DIRECT 物理接口 ADR](docs/ADR/0005-bind-windows-direct-to-physical-interface.md)
- [Windows 选择性合成 DNS ADR](docs/ADR/0006-use-selective-synthetic-dns-on-windows.md)
- [Windows WFP 明文 DNS 截获 ADR](docs/ADR/0007-intercept-windows-dns-with-wfp.md)
- [Windows UDP/QUIC 数据报搬运 ADR](docs/ADR/0008-divert-windows-udp-datagrams.md)
- [Windows 系统组件发行 ADR](docs/ADR/0009-distribute-windows-system-components.md)
- [AI/工程协作规则](AGENTS.md)

## 本地工具链

macOS 开发机可以把固定版本工具安装到仓库自己的 `.tools/`，不会改动系统默认工具链：

```bash
./scripts/bootstrap/install-local-tools.sh
source ./scripts/bootstrap/env.sh
./scripts/bootstrap/check-prerequisites.sh
```

已有符合版本要求的全局 .NET、Node.js 和 pnpm 会直接复用。Rust、Buf、Protobuf Compiler 和 just 使用校验过 SHA-256 的固定版本。

## 常用命令

```bash
source ./scripts/bootstrap/env.sh
just check-tools
just generate
just contracts
just format-check
just lint
just test
just native-bridge-smoke
just gateway-bundle-smoke
just check
```

只有当前已经建立的语言工作区会运行。随着各组件落地，根任务保持同一入口并逐步收紧门禁。

契约基线提交后，可以在修改 `.proto` 文件时运行：

```bash
just contracts-breaking
```

## 平台验收边界

- macOS Provider 必须在真实 System Extension 环境验证。
- Windows WFP/Driver 必须在真实 Windows/WDK 环境验证。
- `DIRECT`/`PROXY` 的完成声明需要路径证据，不能只使用配置或单元测试。
- 签名、Notarization、Driver signing 和独立机器安装属于发布门禁。
