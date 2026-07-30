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
