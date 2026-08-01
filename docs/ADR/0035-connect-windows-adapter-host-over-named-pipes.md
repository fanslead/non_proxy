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
2. gRPC 监听 `\\.\pipe\NonProxy.Adapter.<UserSid>`。SID 取自当前进程 token，必须是
   规范普通用户 SID；Session 0、SYSTEM、LocalService、NetworkService 和服务 SID 均拒绝。桌面端从
   当前 Windows 用户生成相同端点，多用户同时登录不会争用全机固定管道。命名管道实现与
   gatewayd 共享受审计的
   `nonproxy-windows-ipc` incoming：首实例独占、拒绝远端客户端、最多 16 个实例、64 项
   accept queue，并限制产品命名空间、名称长度和 SDDL 输入。
3. 桌面端使用独立 `IAdapterChannelFactory` 和 `FileAdapterCapabilityProvider`。控制面与
   Adapter 只共享通用命名管道客户端实现，不共享端点、令牌或 RPC 服务。
4. SDDL 由当前 SID 确定性生成，只授予 SYSTEM、Administrators 和该用户完整访问；环境覆盖
   只有与该精确 SDDL 完全相等时才接受，不能退回 `Interactive Users`。运行身份中的包指纹
   在 Windows 默认取当前 adapter-host 可执行文件的 SHA-256，供后续升级就绪检查使用。
5. 平台能力失败关闭：Surge 只在 macOS 宣称支持；Windows sing-box 在没有安全重载实现前
   不宣称 `HOT_RELOAD`；Mihomo 继续使用唯一 loopback controller 的公开 HTTP 重载。
6. 发布包把 adapter-host 作为固定发布者签名入口纳入清单。管理员安装器登记一个
   `NonProxyAdapterHost` 登录任务：触发器不指定 `UserId`，principal 使用内置 Users group
   SID `S-1-5-32-545`、`Group` logon、`Limited` run level，实例策略为 `Parallel`。因此任意
   普通用户登录时都在自己的交互 token 和 Session 中获得一个宿主，多用户不会共享进程。
7. 桌面端只从管理员保护的安装注册表读取当前版本目录和 SHA-256，并复验
   `%ProgramFiles%\NonProxy\system\<version>\adapter\nonproxy-adapter-host.exe` 无重解析点且
   哈希一致后，以当前真实用户即时启动。它不继承开发用 Adapter 环境覆盖；已有受信旧版本
   宿主在升级中的当前会话继续服务，任务定义切到新版本后在下次登录自然收敛，避免强杀正在
   执行的配置事务。卸载会撤销任务并只终止受保护版本目录内的精确 adapter-host 路径，覆盖
   计划任务和桌面补启动两种实例，不按进程名误杀外部程序。

## 当前边界

- x64/ARM64 交叉编译与可移植单元测试只能证明 W0 源码边界。
- Windows 发布包、登录任务、桌面即时启动、升级任务切换和卸载源码已接线；真实任务 group
  activation、多用户并行、进程退出、管道 DACL、会话隔离、错误令牌和第三方客户端重载仍需
  Windows 验收。
- 现有应用规则投影要求 macOS `.app` Bundle 路径。Windows 精确可执行文件或 package
  selector 必须通过后续版本化契约实现，不能为接通传输而静默扩大规则范围。

## 后果

- Windows 页面和宿主具备独立、可认证的本地传输源码，不再依赖 Unix socket。
- 用户级第三方配置权限不会被并入 SYSTEM gatewayd。
- 产品仍会在宿主生命周期未就绪时明确失败，不把交叉编译或管道创建等同于可用或已直连。

## 参考

- [Microsoft：ILogonTrigger](https://learn.microsoft.com/windows/win32/api/taskschd/nn-taskschd-ilogontrigger)
- [Microsoft：Principal.LogonType](https://learn.microsoft.com/windows/win32/taskschd/principal-logontype)
