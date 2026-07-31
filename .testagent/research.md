# 桌面端首批测试研究

## 范围

- `NonProxy.Desktop.Core` 的平台抽象、依赖注入组合根和首个 Dashboard。
- `NonProxy.Desktop.Mac` 与 `NonProxy.Desktop.Windows` 仅作为薄启动宿主。
- 当前仓库没有既有 .NET 测试基线，本批次建立 xUnit 与 Avalonia Headless 基线。

## 已确认约束

- 测试框架使用 xUnit v3 3.2.2，与 Avalonia Headless 12.1.1 的 xUnit v3 扩展保持同一主版本。
- Avalonia Headless 与应用使用相同的 12.1.1 版本。
- ViewModel 测试不启动 Avalonia runtime。
- AXAML 仅用一个有价值的无头渲染测试验证绑定和控件加载。
- 测试项目必须同时加入根 solution 和桌面 solution。

## 风险清单

- 同一 ViewModel 可能错误读取运行时操作系统，造成 Windows 复用失败。
- 默认状态可能把未安装的平台组件显示成已生效。
- 组合根可能漏注册平台能力，直到用户操作时才失败。
- AXAML 可能编译通过但初始化或绑定失败。
- 测试程序集可能存在但没有被 solution 或测试运行器发现。

## Windows 本地传输与 Service 批次

### 限定目标

- Rust：`nonproxy-windows-ipc` 的安全命名管道工厂、`gatewayd` 的 Windows 配置、管道监听、Service 状态和 server ready 生命周期。
- C#：`LocalControlEndpoint`、Windows 命名管道控制工厂、Windows 组合根和 `ControlTransportTests`。
- 不包含 WFP、Driver、安装器和真实 Windows 网络栈验收。

### 既有约定

- Rust 使用源模块内 `#[cfg(test)]` 单元测试，测试名为 snake_case。
- .NET 使用 xUnit v3，测试名表达行为，边界组合优先使用 `[Theory]`。
- `NonProxy.Desktop.Tests` 当前只引用 Core；Windows 宿主的 internal 工厂和组合根不能在不改变生产可见性或项目依赖的前提下直接单测。

### 静态配对分析

- 已按技能要求运行一次 polyglot `find-untested-sources`，临时安装 `tree-sitter-language-pack` 后成功生成 JSON。
- 分析器错误纳入了仓库内 `.tools/cargo/registry`，导致 Rust 短符号产生大量第三方误配；其结果只作为静态启发式，不作为行/分支覆盖证据。
- `LocalControlEndpoint` 已配对到 `ControlTransportTests.cs`；Windows-only 源仍需以 target compile 和真实 Windows 测试补足。

### 验收清单

- “仅审查当前未提交的 Windows 传输与 gateway Windows Service 批次”：只修改限定测试及 `.testagent` 证据。
- “补充必要且可在当前 macOS 或 Windows target compile 验证的高价值测试”：覆盖管道命名空间/长度、传输互斥、生产 DACL 门禁、实例上限、Service 状态字段和 ready 时序。
- “测试注释中文 UTF-8”：新增测试没有英文人工注释，测试文本为 UTF-8 中文。
- “不要修改生产业务代码”：只增加测试模块/测试方法，不改变生产分支行为。
- “不得触碰或 stage `.agents/`、`skills-lock.json`，不要提交 Git”：不修改、不暂存、不提交。

## 出口健康测试批次

## 用户目标

- “按照方案出一份完整的技术实现文档，代码仓库要求使用monorepo模式，方便后面加上windows方案。然后代码结构要清晰易懂，不能全都耦合在单文件中。”
- “按照方案规划计划然后实现项目，直到做完。然后分批次提交git，每次提交git之前都需要review一下确认没有任何问题。”

本批次只处理完整产品中的一个闭环：用户显式测试某个代理出口，网关执行真实的代理 TCP 握手，桌面端展示本次结果、耗时和检查时间。该结果不得表述为公网出口 IP 或最终策略路径已经验证。

## 有界目标清单

| 层 | 目标 |
| --- | --- |
| Protocol | `OutboundSummary`、`TestOutboundRequest/Response` |
| Gateway | `control_service.rs`、`control_mapping.rs`、`gateway.rs` |
| Gateway 新模块 | 出口健康状态注册表、出口探测编排 |
| Desktop RPC | `IControlRpcClient`、`GrpcControlRpcClient` |
| Desktop service | `IOutboundService`、`GatewayOutboundService`、控制模型 |
| Desktop UI | `OutboundsViewModel`、`OutboundsView` |
| Tests | gateway 模块/RPC、桌面 service/view-model/headless view |

## 已有约定

- Rust 使用内联 `#[cfg(test)]` 单元测试和 `services/gatewayd/src/control_service/tests.rs` RPC 测试；生产 Rust 不使用 `unwrap`/`expect`。
- .NET 使用 xUnit，异步测试通过 `TestContext.Current.CancellationToken` 传播取消。
- Avalonia UI 使用 `Avalonia.Headless.XUnit` 验证共享视图在 macOS 与 Windows 组合根中均可渲染。
- 控制 RPC 的鉴权上下文由 `OperationContextProvider` 注入；业务失败优先通过 `ErrorDetail` 返回稳定错误码。
- 现有 `nonproxy-outbound` 已实现 HTTP CONNECT/SOCKS5 握手，网关数据面通过 `flow_server::outbound_factory::load_connector` 读取当前出口及系统凭据库。

## 静态源文件配对结果

使用 `find_untested_sources.py` 对仓库根执行 C#/Rust 静态配对。有效扫描结果为：

- `OutboundsViewModel.cs` → `OutboundsViewModelTests.cs`
- `GatewayOutboundService.cs` → `GatewayOutboundServiceTests.cs`、`tests/control-smoke/Program.cs`
- `control_service.rs` → `services/gatewayd/tests/gateway.rs`、`services/gatewayd/tests/provider_rpc.rs`

扫描器也遍历了仓库内 `.tools/cargo/registry`，因此全仓统计受到工具缓存噪声影响；这里仅使用目标文件的确定配对结果。该结果是标识符/导入静态启发式，不是行覆盖率或分支覆盖率证据。

## 参数与边界

- `outbound_id`：必须能构造 `OutboundId`，且当前数据库中存在。
- `timeout`：缺省 5 秒；只接受 1 至 30 秒，纳秒必须是合法 protobuf duration。
- 探测目标：服务端固定的无用户数据 TCP 目标；不接受客户端传入目标。
- 健康缓存：只对相同出口 revision 生效，超过 60 秒变回“未验证”。
- 延迟：记录完整代理握手耗时；失败不返回延迟。
- 错误：不得包含用户名、密码、凭据引用或原始配置；UI 以可操作中文解释稳定错误码。

## 依赖与测试替身

- 网关探测编排通过内部异步探测函数注入测试结果，避免新单元测试绑定端口或访问外部网络。
- HTTP CONNECT/SOCKS5 的字节级握手继续由 `nonproxy-outbound` 既有连接器测试覆盖。
- .NET 通过 `StubControlRpcClient` 和 `RecordingOutboundService` 验证请求、映射及 UI 状态替换。
