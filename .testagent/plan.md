# 桌面端首批测试计划

| 测试 | 目标 |
| --- | --- |
| `PlatformInformationUsesInjectedDisplayName` | 证明共享 ViewModel 通过接口复用 macOS 与 Windows 平台信息 |
| `InitialStateHasHonestUnconfiguredValues` | 证明初始状态不虚报规则、证据或流量接管 |
| `MissingInstallerThrowsDuringComposition` | 证明缺少平台安装能力时在组合阶段快速失败 |
| `CompletePlatformServicesResolvesShell` | 证明完整平台注册可以解析唯一共享 Shell |
| `InitialStateRendersStatusHeadline` | 证明 Dashboard AXAML 在无头环境加载并呈现初始状态 |
| `CompositionRootResolvesBoundMainWindow` | 证明组合根选择注入构造器，并正确加载共享主窗口 |
| solution `--list-tests` | 证明测试项目被 solution 和测试运行器发现 |

## 执行顺序

1. 生成并还原 solution。
2. 运行每个测试类的窄范围测试。
3. 构建完整桌面 solution。
4. 执行测试发现和完整测试。
5. 复查断言质量、遗漏分支和测试隔离。

## Windows 本地传输与 Service 测试计划

| 清单项 | 计划测试或阻塞 |
| --- | --- |
| 管道端点必须有能力文件且只能选择 UDS/Named Pipe 之一 | `EndpointRequiresCapabilityAndExactlyOneLocalTransport` |
| Windows 环境变量、产品私有命名空间和长度边界 | `WindowsEnvironmentOverridesDefaultDirectoryAndPipe`、`WindowsEndpointAcceptsMaximumLengthPipeAndRejectsLongerValue`、`WindowsEndpointRejectsPipeOutsideProductNamespace` |
| gateway 配置必须区分控制/数据管道并限制生产 DACL | `accepts_distinct_private_pipes_and_installer_sddl`、`development_sddl_cannot_start_windows_service`、无效名称/SDDL 测试 |
| 安全描述符在 FFI 前拒绝空值/NUL | Windows-target `rejects_empty_or_embedded_nul_sddl_before_ffi` |
| 命名管道实例数遵守 Windows 1..=254 | Windows-target `rejects_instance_limits_outside_windows_range_before_binding` |
| Service 只在 Running 接受 STOP/PRESHUTDOWN，pending/failed 字段正确 | Windows-target 三个 `windows_service::tests` |
| server ready 必须在监听和运行身份就绪后发送 | macOS 实跑 `server::tests::serves_status_over_private_unix_socket_and_cleans_up` 增加 ready 断言 |
| Windows 控制工厂错误码、连接取消和组合根最终解析 | 阻塞：测试项目不引用 Windows 宿主且目标类型为 internal；需后续 Windows 专属测试项目或受控 `InternalsVisibleTo` |

### 执行顺序

1. macOS 定向运行配置、server 生命周期和 C# 控制传输测试。
2. Windows x64/arm64 target compile 全部 Rust 测试代码。
3. 运行完整 `nonproxy-gatewayd` 与 Desktop Tests。
4. 执行格式、diff check、断言强度和测试缺口复核。

## 出口健康测试批次

## 验收清单与测试映射

| 验收项 | 计划测试 |
| --- | --- |
| 鉴权后才能发起出口探测 | `test_outbound_requires_the_exact_session_capability` |
| 合法请求执行代理握手并返回耗时 | `successful_probe_reports_latency_and_updates_current_health` |
| 超时被限制在 1 到 30 秒且错误稳定 | `probe_rejects_out_of_range_timeout`、`timed_out_probe_returns_retryable_error` |
| 出口 revision 变化后旧健康结果不可复用 | `health_requires_current_revision_and_fresh_observation` |
| 过期健康结果恢复为“未验证” | `health_requires_current_revision_and_fresh_observation` |
| 列表返回健康状态、检查时间和握手耗时 | `list_outbounds_returns_fresh_probe_observation`、`ListMapsFreshHealthObservation` |
| 桌面 RPC 带会话能力和固定探测超时 | `TestOutboundBuildsAuthenticatedBoundedRequest` |
| 失败映射为可操作中文且不夸大为公网验证 | `TestMapsHandshakeFailureToActionableMessage` |
| ViewModel 只替换被测试行并显示耗时/时间 | `TestCommandUpdatesOnlySelectedOutbound` |
| macOS/Windows 共享界面都显示逐行测试入口 | `MacCompositionRendersOutboundTestAction`、`WindowsCompositionRendersOutboundTestAction` |

## 实现阶段

1. 扩展向后兼容的出站摘要协议字段。
2. 新增独立健康注册表和探测编排模块，接入 Gateway 与 ControlService。
3. 扩展桌面 RPC/service/model。
4. 扩展 ViewModel 和共享 Avalonia 视图。
5. 完成窄范围测试、质量审查、全仓相关门禁。
6. 执行 macOS Release universal Native Messaging Host 与完整桌面 solution 打包门禁。

## 默认代理原子发布批次

| 验收项 | 计划测试 |
| --- | --- |
| 新库升级后默认 direct/revision 1 | migration 与 `initial_route_is_direct_at_revision_one` |
| 路由和 pending snapshot 同事务 | `proxy_route_and_snapshot_are_staged_atomically` |
| revision 冲突无部分写入 | `stale_revision_changes_neither_route_nor_snapshot` |
| 缺失、禁用或 TCP-only 出口拒绝 | `missing_disabled_or_incompatible_default_outbound_is_rejected` |
| 已选默认出口不能被停用或降为 TCP-only | `active_default_outbound_cannot_be_disabled_or_limited_to_tcp` |
| 已有 pending 时路由回滚 | `pending_snapshot_rolls_back_the_route_update` |
| 回滚源无效时路由设置也回滚 | `invalid_rollback_source_rolls_back_the_route_update` |
| 默认 PROXY 进入 snapshot payload | `selecting_default_proxy_stages_a_proxy_default_snapshot` |
| 历史回滚恢复默认路由 | `rollback_restores_the_source_snapshot_default_route` |
| RPC 鉴权、状态与目录一致 | ControlService 默认路由测试 |
| C# 请求携带 context 与 revision | `SetDefaultRouteBuildsAuthenticatedOptimisticRequest` |
| 跨页 revision 变化拒绝 | `ListRejectsRoutingRevisionChangeAcrossPages` |
| ViewModel 接受后刷新并保留 pending 语义 | `SetDefaultReloadsOnlyAfterServerAcceptsPendingSnapshot` |
| 一键恢复默认直连使用同一 revision/ACK 语义 | Rust/C# direct route 与 ViewModel 测试 |
| macOS/Windows 共享 UI 都有入口 | 两个平台 headless view 测试 |

### 执行顺序

1. 迁移、仓储和事务原子性测试。
2. Gateway 默认决策、回滚和 Control RPC 测试。
3. 生成 C#/Swift 契约。
4. Desktop RPC/service/ViewModel/headless UI 测试。
5. 契约兼容、格式、lint、全仓测试与双平台打包门禁。
6. 对 diff、错误语义、断言质量和剩余缺口做提交前 review。

## 决策与路径证据批次

| 验收项 | 计划测试 |
| --- | --- |
| 决策批量写入幂等且按时间倒序分页 | storage repository integration tests |
| 记录保存应用、目标、规则、动作和证据，不保存 URL/秘密 | 构造器与持久化负向测试 |
| Provider 必须鉴权并限制单批数量 | Provider RPC tests |
| 决策引用的快照必须存在且内容一致 | Gateway ingestion tests |
| PATH 级 DIRECT 必须有物理接口 | 证据语义负向测试 |
| PATH 级 PROXY 必须有匹配出口 | 证据语义负向测试 |
| 控制面只读分页返回稳定活动模型 | Control RPC 与 C# service tests |
| 活动页明确显示证据等级和实际路径 | ViewModel/headless UI tests |
| 首页最近决策数来自权威存储 | System status service tests |
| 平台只在路径建立后提升到 PATH | Swift 与 Windows 数据面行为测试 |

### 执行顺序

1. 完成 storage 模型、V8 migration 和 repository 测试。
2. 完成 Provider ingestion 与快照/证据验证。
3. 完成 Control RPC、契约生成和桌面映射/UI。
4. 完成 macOS/Windows 生产者与批处理边界。
5. 执行跨语言 E2E、全仓测试、打包和提交前 review。

## macOS/Windows 真实路径生产器批次

| 验收项 | 计划测试或检查 |
| --- | --- |
| fail-open 只能匹配显式 OPEN 代理策略 | storage migration/repository、Swift observation、Rust observation 负向测试 |
| macOS 代理预建立失败可安全把未打开 flow 交给 DIRECT | `ProxySetupRecoveryPlannerTests`、`RelaySetupObserverTests`、Transparent 全量测试 |
| macOS DNS 只在非缓存真实解析后声明 PATH | `DNSQueryCoordinatorTests` 的 direct/proxy/fail-open/cache 分支 |
| Windows TCP/UDP/DNS 在实际就绪点生成路径 | Windows x64/arm64 全 workspace target check 与 arm64 clippy |
| 证据失败不影响转发 | Windows report helper 改为无返回 best-effort，交叉编译验证全部调用点 |
| 上报队列有界、重试不换批次 | `ProviderDecisionReporterTests` |
| 响应丢失后的批次重放不重复统计 | gateway telemetry 与 Provider RPC 重放测试 |
| Provider 并发请求允许窗口内乱序 | `accepts_bounded_out_of_order_sequences_but_rejects_old_or_duplicate_values` |
| 丢失计数进入系统诊断且不夸大网络状态 | `GatewayDiagnosticsServiceTests` |
| 契约、生成物和跨语言实现一致 | C#/Swift generation check 与 Buf breaking check |

### 执行顺序

1. 扩展 V9 fail-open 证据约束并完成三层负向验证。
2. 实现 macOS Transparent/DNS 与 Windows TCP/UDP/DNS 的真实路径生产点。
3. 增加有界 reporter、批次幂等、滑动防重放窗口和丢失诊断。
4. 运行 Rust/Swift/.NET 定向与全量测试。
5. 运行 Windows x64/arm64 target check、格式、lint、契约和 Release Bundle 门禁。
6. 逐文件 review、显式暂存、复查 staged diff 后提交本批。

## macOS gatewayd 防回环信任边界批次

| 验收项 | 计划测试或检查 |
| --- | --- |
| 每个发布快照都包含唯一 gatewayd 系统直连规则 | snapshot builder / gateway integration tests |
| 系统规则高于用户默认代理且只匹配固定 macOS 身份 | Rust PolicyEngine 行为测试、Swift 快照现有回归 |
| 用户不能伪造 SYSTEM/BUILT_IN/ADAPTER/SUBSCRIPTION 来源 | Control RPC 与 Gateway 负向测试 |
| 用户不能占用保留系统策略 ID | Gateway 负向测试 |
| gatewayd 裸二进制签名标识跨构建稳定 | macOS Bundle verifier 与 Release 构建 |
| 正式 TeamIdentifier 与宿主、LaunchAgent、策略 signer 一致 | 打包/校验脚本、PolicyEngine 负向测试 |
| 旧 pending 原子替换，失败不留下半状态 | storage repository 与 gateway 启动测试 |
| 旧 active 保留到升级快照 ACK | gateway 启动测试 |
| 旧 active 期间不建立任何代理上游 | readiness 原子门、连接工厂和 ACK 激活测试 |
| 历史回滚重建当前系统规则 | gateway 回滚集成测试 |
| Windows 自身 PID 防回环保持不受影响 | Windows target check 与既有 WFP 配置测试 |
| 文档不再把未落地约束写成既成事实 | 技术实现文档与 README diff review |

### 执行顺序

1. 新建隔离的 `system_policies` 模块，集中定义保留身份、规则和用户写边界。
2. 快照构建时追加系统规则并覆盖哈希、payload、启动升级、回滚和 Provider
   重编译链路。
3. 固定 macOS gatewayd 代码签名 identifier；正式签名提取 TeamIdentifier 写入
   LaunchAgent，并校验宿主 App、gatewayd 二进制、LaunchAgent 和快照 signer
   四处身份一致。
4. 增加行为/负向/升级原子性测试，并修正受影响的快照 policy count 和运行目录
   断言。
5. 运行 Rust、Swift、.NET、Windows target、格式、lint、契约和 Release 门禁。
6. 提交前逐文件 review，显式暂存并复查 staged diff。
