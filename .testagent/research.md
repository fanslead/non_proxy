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
