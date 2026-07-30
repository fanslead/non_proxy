# NonProxy

NonProxy 是一个本地优先的跨平台智能分流网关。用户选择应用或网站后，系统把匹配流量明确路由为 `DIRECT` 或 `PROXY`，并保留决策、路径和出口证据。

当前状态：按 `docs/TECHNICAL_IMPLEMENTATION.md` 实施中。仓库内的构建通过不代表 macOS System Extension、Windows WFP 或真实网络路径已经验收。

已经落地的基础包括版本化契约、Rust 策略模型/编译器/SQLite 权威存储、认证 UDS 控制面、Avalonia 共享产品工作区，以及 macOS Swift Provider 的快照校验、缓存、纯函数决策和 Provider RPC 客户端。Transparent Proxy 已具备真实 Provider 入口、来源签名身份、TCP/UDP 目标解析、绑定物理接口的 DIRECT relay、BLOCK 与显式失败模式；代理出口 relay、DNS Proxy、浏览器学习与 Windows WFP 仍按里程碑继续实现。尚未启用的数据路径和 RPC 会明确失败，不伪造成功。

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
