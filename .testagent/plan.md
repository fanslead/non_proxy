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
