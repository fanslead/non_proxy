# ADR-0001：桌面 UI 统一使用 Avalonia

- 状态：Accepted
- 日期：2026-07-30
- 决策范围：macOS 与 Windows 桌面控制面

## 背景

原技术方案为：

- macOS 主应用使用 SwiftUI。
- Windows 主应用使用 C# + WinUI 3。
- Rust 共享策略、网关和领域核心。

这种方案能获得最原生的平台 UI，但会形成两套页面、ViewModel、交互状态和自动化测试。NonProxy 的主要界面是状态、策略列表、应用选择、网站学习、节点配置、日志和诊断，不依赖复杂的原生媒体或高性能自定义控件。该类控制面适合复用同一套跨平台 UI。

Avalonia 12 支持 Windows 和 macOS，共享 C#/AXAML UI；官方提供 macOS/Windows 系统托盘和 NativeMenu。Avalonia 只统一界面层，并不抽象 Network Extension、System Extension、WFP、Driver、安装、签名或系统权限。

参考：

- [Avalonia supported platforms](https://docs.avaloniaui.net/docs/supported-platforms)
- [Avalonia macOS platform guide](https://docs.avaloniaui.net/docs/platform-specific-guides/macos)
- [Avalonia TrayIcon](https://docs.avaloniaui.net/controls/navigation/trayicon)
- [Avalonia native platform interop](https://docs.avaloniaui.net/docs/app-development/native-interop)
- [.NET support policy](https://dotnet.microsoft.com/en-us/platform/support/policy)

## 决策

桌面 UI 统一使用：

- Avalonia 12。
- .NET 10 LTS。
- C#。
- AXAML。
- CommunityToolkit.Mvvm。

仓库只保留一套桌面 UI 实现，并使用两个极薄的平台启动宿主：

```text
apps/desktop/NonProxy.Desktop.Core
apps/desktop/NonProxy.Desktop.Mac
apps/desktop/NonProxy.Desktop.Windows
```

`Core` 包含全部 AXAML、ViewModel、主题、验证和导航；两个宿主只负责 TFM、应用生命周期、系统权限和打包入口，不得复制页面。

- `NonProxy.Desktop.Mac`：`net10.0-macos`，从最终包含 System Extension 的 `.app` 进程提交激活/卸载请求。
- `NonProxy.Desktop.Windows`：`net10.0`，调用 Windows Service/Installer。

平台高权限实现继续独立：

- macOS：Swift 的 Transparent Proxy、DNS Proxy、原生宿主桥和签名打包；Avalonia 宿主通过固定 C ABI 调用原生桥，由原生桥使用 SystemExtensions/NetworkExtension API 完成安装控制。
- Windows：Windows Service、WFP Controller、必要时的最小 Callout Driver 和安装签名。

Avalonia UI 不直接调用 NetworkExtension 或 WFP。它通过版本化控制契约与 `gatewayd` 通信；需要安装、授权或卸载系统组件时，通过平台桥接服务调用。

## 目标结构

```text
apps/desktop/
├── NonProxy.Desktop.Core/
│   ├── App/
│   ├── Features/
│   ├── Views/
│   ├── ViewModels/
│   ├── Controls/
│   ├── DesignSystem/
│   ├── Services/
│   ├── Platform/
│   └── Assets/
├── NonProxy.Desktop.Mac/
├── NonProxy.Desktop.Windows/
├── NonProxy.Desktop.Tests/
└── NonProxy.Desktop.E2E/
```

平台差异只能通过小接口进入 UI：

```text
IPlatformShell
ISystemComponentInstaller
IAutoStartService
INativeFilePicker
INotificationService
IAppDiscoveryService
```

平台实现不得包含规则编译、数据库、协议网关或页面业务逻辑。

## 后果

### 正面

- 页面、ViewModel、验证、主题和大部分 UI 自动化只维护一份。
- macOS 与 Windows 功能更容易保持一致。
- C# 控制客户端可直接使用生成的 Protobuf 契约。
- 系统托盘、原生菜单和常规桌面控件已有跨平台支持。
- 后续增加 Linux 控制面时无需重写主体 UI。

### 代价

- Avalonia 控件由 Skia 绘制，不等同于原生 SwiftUI/WinUI 控件。
- macOS 的完整系统 API 仍需要 Native Bridge；Avalonia 不替代 Swift System Extension。
- 包体和常驻内存通常高于单纯原生 SwiftUI。
- macOS 14/15 在当前 Avalonia 支持矩阵中属于 Tier 2，必须由项目自己承担完整回归测试；macOS 26 为 Tier 1。
- 平台菜单、托盘、文件选择、开机启动和辅助功能仍需分别验收。

## 未采用方案

### SwiftUI + WinUI 3

原生体验最好，但需要两套 UI、ViewModel 和测试，不符合统一控制面的维护目标。

### .NET MAUI

桌面支持以 Windows 和 Mac Catalyst 为主；NonProxy 需要传统 macOS 桌面菜单、托盘和独立 System Extension 打包，Avalonia 的桌面模型更合适。

### Web UI/Tauri/Electron

可以跨平台，但会引入浏览器运行时、安全边界和额外 Native Bridge；对于高权限网络工具，Avalonia 的本地 .NET 桌面进程更直接。

## 约束

- “统一 UI”不得被表述为“完全没有平台代码”。
- System Extension、WFP 和安装签名仍然保留平台目录。
- ViewModel 不得直接判断 `OperatingSystem.IsMacOS()` 并堆积平台分支；通过小型平台接口注入。
- 公共页面必须在 macOS 和 Windows 都有 UI 自动化覆盖。
- macOS 14/15 的支持状态必须在发布文档中如实说明，并由项目 CI/设备矩阵补足。

## 复审条件

出现以下情况时重新评估：

- Avalonia 无法满足系统托盘、辅助功能或输入法的发布门槛。
- macOS Tier 2 缺陷无法通过项目维护或商业支持解决。
- UI 性能或内存连续两个版本无法达到预算。
- 平台专有页面超过全部页面的 30%，共享 UI 的收益明显下降。
