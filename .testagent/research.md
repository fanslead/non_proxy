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

## 默认代理原子发布批次

### 缺口与根因

- 产品默认模型要求“默认 PROXY，应用/网站例外 DIRECT”，但快照构建器此前把
  `default_decision` 固定为 `DIRECT`，因此直连规则无法形成例外语义。
- 出口导入与握手测试只证明配置和连接器可用，没有权威的“选择默认出口”配置。
- 历史快照回滚返回值也固定声明 `DIRECT`，与 payload 内的真实默认决策可能不一致。

### 设计约束

- `routing_settings` 是单例权威配置，初始为 direct/revision 1，升级不得自动改变
  既有用户网络路径。
- 默认路由更新和 pending snapshot 必须共享一个 SQLite Immediate 事务。
- 默认代理必须引用存在、启用且可承载完整网关的出口，并经 Compiler 再次校验；
  默认 PROXY 使用 fail-closed。
- 桌面端使用 routing revision 乐观并发，RPC 成功只表述为 pending ACK。
- 跨页出口目录必须携带稳定 revision，且最多一个 `is_default`。
- 回滚必须从历史 payload 解码默认 decision，并原子同步权威默认路由。
- 当前默认出口不得被后续批量导入停用或改成完整网关不支持的 TCP-only 类型。
- UI 必须提供恢复默认直连的安全退路；该操作不能绕过快照发布与 Provider ACK。

### 静态配对

- `routing_settings_repository.rs` → `routing_settings_repository.rs` integration tests。
- `routing_gateway.rs`、`gateway.rs` → `services/gatewayd/tests/gateway.rs`。
- `control_service.rs` → `control_service/tests.rs`。
- `GatewayOutboundService.cs` → `GatewayOutboundServiceTests.cs`。
- `OutboundsViewModel.cs`、`OutboundsView.axaml` →
  `OutboundsViewModelTests.cs`、`OutboundsViewTests.cs`。

## 决策与路径证据批次

### 缺口与边界

- Provider 契约已有 `ReportDecisionBatch`，V1 数据库也预留
  `connection_decision`，但 RPC 当前固定返回 `NP_FEATURE_NOT_AVAILABLE`，平台端没有
  上报器，桌面“活动记录”和首页最近决策数仍使用空实现。
- 代理握手健康只证明节点可连接；快照 `ACTIVE` 只证明 Provider 已加载策略。二者
  都不能替代某条用户连接的决策、物理接口或代理出口证据。
- Provider 上报属于高权限但仍不应被盲信：gatewayd 必须校验会话、批量上限、
  字段边界、证据语义和所引用快照，再持久化有界、可清理的元数据。
- 活动页必须区分 `DECISION`、`PATH`、`EXIT`，不能把仅有策略命中的记录显示为
  “已确认直连”。

### 实现切片

1. 追加 V8 migration 和专用 decision repository，覆盖幂等写入、倒序分页和保留。
2. 启用 Provider 批量上报，拒绝畸形、跨能力和快照不一致的记录。
3. 增加只读 `ListConnectionDecisions` 控制 RPC，接入桌面活动页和首页计数。
4. macOS Transparent/DNS Provider 与 Windows 数据面在实际路径建立后批量上报。
5. 出口探针只在独立探针观察到公网结果时提升到 `EXIT`，本批不得伪造。

## macOS/Windows 真实路径生产器批次

### 证据边界

- `DECISION` 只证明策略判定；`PATH` 只能在代理通道、物理 TCP 连接或绑定物理
  接口的 UDP/DNS 路径实际就绪后产生；DNS 缓存命中不得冒充新的网络路径。
- `PROXY + failure_mode=OPEN` 失败后实际建立 DIRECT 时，必须保留原始代理决策，
  同时显式记录 `fail_open_direct`、物理接口和稳定错误码，不能改写成普通 DIRECT。
- `EXIT` 仍要求独立、受控的公网出口探针；本批没有新增虚构的出口证据。
- 证据链是 best-effort 旁路：编码失败、队列饱和或 reporter 不可用只增加丢失
  诊断，不能改变 TCP/UDP/DNS 的转发结果。

### 提交前审查发现

- Provider 并发的 DNS、心跳和决策 RPC 可能乱序到达；严格单调的服务端序号检查会
  误杀合法请求，因此改为 4096 条有界滑动窗口，仍拒绝重复和过旧序号。
- 决策批次在“服务端已写入、响应丢失”后重试时，记录本身幂等但丢失计数原先会
  重复累加；新增稳定 `batch_id` 和有界批次历史后，统计也保持幂等。
- fail-open 后的 DIRECT DNS 若命中缓存，没有发生新网络路径；记录为带失败原因的
  `DECISION`，而不是 `PATH`。
- Windows 事件 ID 不需要密码学随机性；改为 Provider 代次内无失败的单调 ID，
  避免系统随机源异常反向阻断数据面。

### 平台验收边界

- macOS Swift 测试和 Universal Release Bundle 构建可以证明源码、双架构产物与
  Bundle 结构；未在本批执行已签名 System Extension 的真实安装和外部 VPN 共存流量。
- Windows x64/arm64 使用本机 SQLite link metadata 完成 `cargo check`/clippy，
  只证明 Windows 条件编译与类型，不证明链接后的 Service、WFP Driver 或真实网络栈。

## macOS gatewayd 防回环信任边界批次

### 审计发现

- 技术文档要求 `gatewayd` 的代理服务器连接由 Transparent Provider 识别为系统
  组件并交还物理网络，但当前快照只包含数据库中的用户策略，没有注入这一系统规则。
  默认 PROXY 时，真实 System Extension 环境存在把 gatewayd 的代理上游连接再次
  送回 gatewayd 的递归风险。
- `MacAppIdentityResolver` 依赖 `sourceAppSigningIdentifier`。当前打包脚本给裸
  `gatewayd` 二进制签名时没有指定 identifier；临时签名得到的 identifier 带内容
  哈希，不能作为跨版本稳定的系统策略身份。
- 低权限 Control `UpsertPolicy` 会完整接受客户端给出的 `source_kind/origin`。
  领域模型只验证二者组合一致，无法阻止客户端伪造最高优先级 `SYSTEM` 策略。
- Windows WFP 配置已携带 gatewayd 自身 PID，由 Driver 侧避免把代理进程再次
  重定向；本批只修复 macOS 的签名身份与快照规则，同时收紧共享控制面写入边界。

### 设计结论

- 裸二进制固定使用 `com.nonproxy.gatewayd` 作为代码签名 identifier，并在 Bundle
  校验脚本中把该值作为发布门禁。正式签名还必须把二进制 TeamIdentifier 原样写入
  LaunchAgent 环境，并在快照匹配器中同时约束 signer；仅有 identifier 的临时签名
  身份不能作为发布安全边界。
- 每次构建快照时在内存中追加 `system-macos-gateway-direct`：匹配 macOS
  `com.nonproxy.gatewayd`，动作固定 DIRECT/fail-closed，来源为 SYSTEM。规则进入
  不可变快照和哈希，但不写用户策略表、不出现在普通策略编辑列表。
- 用户写接口只允许用户可编辑来源和 `origin=USER`，同时拒绝保留系统 ID；内部
  系统规则由可信快照构建器产生，不能经低权限 RPC 创建或覆盖。
- 启动必须在 Provider 控制面绑定前检查旧 pending/active payload：旧 pending 的
  拒绝与重建快照写入保持同一事务，旧 active 在新快照 ACK 前继续服务。重建使用
  候选 payload 而不是数据库草稿，避免把未发布编辑意外带入升级。
- 仅保留旧 active 仍存在启动竞态：已运行 Provider 可能用缓存旧快照在控制面拉取
  升级 pending 前先发起 flow。gatewayd 因此必须把“当前 active 含受保护规则”
  保存为进程内原子门；TCP/UDP、代理 DNS 和出口探测共用的连接工厂在该门开启前
  返回稳定可重试错误，不能读取凭据或创建代理上游。
- 回滚不能复制可能缺少或包含旧 signer 的历史 payload；应保留历史策略、能力和
  默认决策，同时重建当前受保护系统规则并在路由事务内记录历史来源。

## 独立签名出口探针批次

### 审计发现

- 既有 `EXIT` 数据结构允许 Provider 携带任意 `exit_probe_id`，但 gatewayd 没有
  验证远端回执；高权限 Provider 仍不能被授权自行把 `PATH` 提升成 `EXIT`。
- 代理握手健康、系统快照激活和平台 PATH 都不能证明公网看到的来源地址。出口
  证明必须由独立远端从 TLS 连接对端取得地址，并返回绑定 nonce、地址、时间和
  固定 key id 的签名回执。
- 探针请求不能携带应用、域名、规则、URL、凭据或由桌面端提供的任意目标；远端
  只应看到随机 nonce、TLS 对端公网地址和请求时间。
- macOS 直连探针依赖已激活的受保护 gatewayd SYSTEM 规则；Windows 直连探针必须
  显式绑定物理默认接口，不能把 gatewayd 的普通系统路由误称为物理直连。

### 设计结论

- 使用 HTTPS + Ed25519 双重身份：TLS 保护传输和域名，安装配置固定公钥验证 JSON
  回执。nonce 为 32 字节随机值，回执最多存活 120 秒并限制未来时钟偏差。
- endpoint 和公钥只能成对出现在 gatewayd 环境配置中；RPC 只接受 DIRECT 或已
  保存的 PROXY 出口，无法传入 URL。
- 探针服务直接在 TLS listener 上读取 peer address，不接受转发头，不记录请求，
  且拒绝非公网地址、畸形 query、过大响应和宽权限密钥文件。
- Provider 上报 `EXIT` 一律拒绝；只有 gatewayd 自己完成 TLS、验签、nonce 和时间
  校验后才能向低权限控制端返回已验证摘要。

### 回执持久化与桌面展示切片

- 探针网络成功不能先于本地权威写入对用户声明 `verified=true`；SQLite 写入失败
  返回稳定、可重试的 `NP_EXIT_PROBE_PERSIST_FAILED`，响应不携带未落库回执。
- V10 独立表只保存路由级探针事实，不伪造与任意用户连接的关联。回执不可更新，
  相同 `probe_id` 精确幂等、冲突拒绝，并按序号保留最近 2048 条。
- 查询协议同时返回历史回执和当前安装能力，保证“未配置探针”和“尚无历史结果”
  是两个不同状态；桌面端未配置时仍可读历史，但不能发起新请求。
- DIRECT 以 `outbound_id = NULL` 隔离，PROXY 必须绑定合法出口 ID；桌面只把各
  路由最新记录附到对应行，删除出口的旧回执不会错误附给其他代理。
- 即时响应和历史查询都要核对 probe id、路由、地址/地址族和时间戳；界面把握手
  健康与签名公网回执拆开，并以“最近签名回执”避免暗示持续有效。

### 公钥轮换与生产部署切片

- 轮换不能让服务端先签新 key；客户端必须先发布 old+new 信任窗口，并按回执
  `key_id` 精确选择公钥。未知 key id、重复 key 和超过四把都 fail closed。
- 旧版单公钥环境变量需要保持兼容，但单值与复数变量同时存在属于配置歧义，
  gatewayd 必须拒绝启动。
- 探针主机直接终止 TLS、直接观察 TCP peer，不可位于 L7/CDN/SNAT 后；systemd
  仅开放绑定低端口能力，并把配置目录保持只读。
- 私钥工具只创建新的 0600 普通文件，不跟随符号链接、不覆盖旧文件、不打印 secret；
  health 只公开当前 key id。管理工具的普通网络验证不是产品路径验收。
- macOS 将 endpoint/keyring 固化进签名 LaunchAgent 并在打包冒烟重新解析；
  Windows Install/Repair 未传参数时保留现值，失败回滚同时恢复旧信任集合。
