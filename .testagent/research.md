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
