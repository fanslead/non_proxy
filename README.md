# NonProxy

NonProxy 是一个本地优先的跨平台智能分流网关。用户选择应用或网站后，系统把匹配流量明确路由为 `DIRECT` 或 `PROXY`，并保留决策、路径和出口证据。

当前状态：按 `docs/TECHNICAL_IMPLEMENTATION.md` 实施中。仓库内的构建通过不代表 macOS System Extension、Windows WFP 或真实网络路径已经验收。

已经落地的基础包括版本化契约、Rust 策略模型/编译器/SQLite 权威存储、认证 UDS 控制面、Avalonia 共享产品工作区，以及 macOS Swift Provider 的快照校验、缓存、纯函数决策和持久化 Provider RPC 客户端。桌面端可以结构化导入 SOCKS5/HTTP CONNECT 出口，密码进入系统凭据库，数据库只保存版本化引用，导入后的能力会参与策略编译。`gatewayd` 已具备受限 UDS 上的 NPF1 TCP/UDP 数据通道、窗口背压和 SOCKS5/HTTP CONNECT 出口连接器。Transparent Proxy 已具备真实 Provider 入口、来源签名身份、TCP/UDP 目标解析、绑定物理接口的 DIRECT relay，以及通过 NPF1 连接所选代理出口的 TCP/UDP relay；BLOCK 与显式失败模式也已接入。Rust DNS 核心已经具备严格消息校验、DIRECT/PROXY/出站/网络分区缓存、正负响应 TTL 处理和 A/AAAA/CNAME 观察提取；`gatewayd` 已接通经过 DNS Provider 身份与活动快照校验的 RPC，支持绑定物理网卡的 DIRECT UDP/TCP、截断回退 TCP、SOCKS5 UDP/TCP 和 HTTP CONNECT TCP。macOS DNS Proxy Provider 已经接入系统 DNS 设置、split DNS 上游选择、应用身份、策略路由、UDP/TCP flow 和有界并发。Debug 与 Universal Release `.app` 会自动嵌入两个真实 System Extension Bundle、Swift 原生宿主桥以及由 `SMAppService` 管理的用户级 `gatewayd` LaunchAgent。UI、后台服务和沙盒 Provider 统一使用 App Group 状态目录；安装事务会等待后台服务的两个私有 Socket 和能力文件就绪后再启用网络配置，UI 退出不会停止它，明确卸载才会撤销登记。打包门禁会校验架构、导出符号、Framework 链接、LaunchAgent 配置、权限声明和签名，跨语言与后台服务冒烟还会验证借用字节、`size_t` 映射、非 ASCII UTF-8、运行时文件权限和 SIGTERM 清理。默认临时签名只证明开发包结构、ABI 和直接启动的后台二进制自洽；Developer ID/provisioning profile、系统审批、真实 `SMAppService` 登记、System Extension 激活和 VPN 共存流量仍需后续系统验收。自动化冒烟也会让 Swift `NWConnection` 经真实 Unix Socket、Rust 数据面和 HTTP CONNECT 夹具完成回显。浏览器学习及 Windows WFP 仍按里程碑继续实现，尚未启用的数据路径和 RPC 会明确失败，不伪造成功。

运行概览会按“后台服务 → 透明代理 → DNS 分流 → 网络接管”显示真实分段状态。等待授权时可通过原生桥直接打开 macOS“登录项与扩展”，允许后重新检查；部分安装可执行修复，卸载需要二次确认。诊断页复用同一组分段证据并显示稳定错误码。

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
