# ADR-0035：Windows Adapter Host 使用独立认证命名管道

- 状态：Accepted
- 日期：2026-08-01

## 背景

Windows 桌面端复用了客户端协同页面，但 adapter-host 只有 Unix UDS 服务端，导致页面只能
失败关闭。adapter-host 会执行用户选择的第三方程序并修改用户配置，不能复用以 LocalSystem
运行的 gatewayd 会话，也不能只靠“本机命名管道”作为授权依据。

## 决策

1. Windows adapter-host 仍是独立的按用户低权限进程。状态目录位于当前用户
   `%LOCALAPPDATA%\NonProxy\adapter-host`，会话使用独立的 32 字节
   `adapter.capability`。
2. gRPC 监听 `\\.\pipe\NonProxy.Adapter.v1`。命名管道实现与 gatewayd 共享受审计的
   `nonproxy-windows-ipc` incoming：首实例独占、拒绝远端客户端、最多 16 个实例、64 项
   accept queue，并限制产品命名空间、名称长度和 SDDL 输入。
3. 桌面端使用独立 `IAdapterChannelFactory` 和 `FileAdapterCapabilityProvider`。控制面与
   Adapter 只共享通用命名管道客户端实现，不共享端点、令牌或 RPC 服务。
4. 开发 SDDL 只用于本地控制台调试。正式启动器必须显式下发绑定当前交互用户的 SDDL；
   缺少安装器/启动器生产安全配置不能当作系统验收证据。
5. 平台能力失败关闭：Surge 只在 macOS 宣称支持；Windows sing-box 在没有安全重载实现前
   不宣称 `HOT_RELOAD`；Mihomo 继续使用唯一 loopback controller 的公开 HTTP 重载。

## 当前边界

- x64/ARM64 交叉编译与可移植单元测试只能证明 W0 源码边界。
- Windows 发布包尚未分发 adapter-host，也未实现按用户登录启动、升级切换、退出和卸载；
  真实管道 DACL、会话隔离、错误令牌和第三方客户端重载仍需 Windows 验收。
- 现有应用规则投影要求 macOS `.app` Bundle 路径。Windows 精确可执行文件或 package
  selector 必须通过后续版本化契约实现，不能为接通传输而静默扩大规则范围。

## 后果

- Windows 页面和宿主具备独立、可认证的本地传输源码，不再依赖 Unix socket。
- 用户级第三方配置权限不会被并入 SYSTEM gatewayd。
- 产品仍会在宿主生命周期未就绪时明确失败，不把交叉编译或管道创建等同于可用或已直连。
