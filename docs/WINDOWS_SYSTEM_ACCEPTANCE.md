# Windows 系统组件与真实网络路径验收

本文是 NonProxy Windows x64/ARM64 发布门禁。构建、交叉编译、单元测试、未签名
WDK 产物、Service 显示 Running 或策略显示 DIRECT，都不能单独代表真实网络
路径验收通过。

## 1. 验收结论分级

| 等级 | 必须证据 | 可以声明 |
|---|---|---|
| W0 | PowerShell 解析、Rust/.NET 编译、单元测试 | 源码门禁通过 |
| W1 | WDK x64/ARM64 构建、INF 校验 | 驱动可构建 |
| W2 | 固定发布者校验、Microsoft 内核签名、干净 VM 安装/卸载 | 系统组件可安装 |
| W3 | Driver Verifier、崩溃/重启/升级回滚 | 生命周期验收通过 |
| W4 | 真实 VPN 下 DIRECT/PROXY 的决策、路径和出口证据 | 指定矩阵网络路径通过 |

产品只可对实际完成 W4 的 OS、架构、VPN 类型和协议组合声明“已确认直连”。

## 2. 发布前置条件

- Windows 10 1903 或更高版本；发布矩阵仍需覆盖 Windows 10 22H2 和受支持的
  Windows 11。
- x64 与 ARM64 分别构建，不在一份包内混用 Driver。
- Visual Studio 2022、Windows SDK 与 WDK。
- PowerShell 7.4 或更高版本；工程脚本不会回退到 Windows PowerShell 5.1。
- 企业代码签名证书，私钥位于受控签名主机或 HSM。
- Microsoft Hardware Dev Center 账号及 Attestation/HLK 流程。
- 可恢复快照的独立 Windows 测试机；不得在开发者唯一工作机启用 Driver
  Verifier。
- 至少一个由团队直接控制、可从 DIRECT 与 PROXY 路径访问的独立出口探针。
  验收记录中不得写真实代理密码、Token 或私钥。

## 3. 生成与签名发布目录

先生成未签名的架构 Driver：

```powershell
.\scripts\windows\build-driver.ps1 -Platform x64
.\scripts\windows\build-driver.ps1 -Platform ARM64
```

将对应架构 Driver 提交 Hardware Dev Center，下载并验证 Microsoft 签名的
`NonProxyWfp.cat`/`NonProxyWfp.sys`。生产包不得用测试证书或关闭 Secure Boot
代替这一步。

发布 UI 和 Service 后，组装一个全新的空目录：

```powershell
dotnet publish .\apps\desktop\NonProxy.Desktop.Windows\NonProxy.Desktop.Windows.csproj `
  -c Release -f net10.0-windows10.0.26100.0 -r win-x64 --self-contained true `
  -o .\.artifacts\desktop\win-x64
cargo build --release --target x86_64-pc-windows-msvc -p nonproxy-gatewayd

.\scripts\windows\build-release-package.ps1 `
  -Version 0.1.0 `
  -Architecture x64 `
  -DesktopPublishDirectory .\.artifacts\desktop\win-x64 `
  -GatewayExecutable .\target\x86_64-pc-windows-msvc\release\nonproxy-gatewayd.exe `
  -DriverDirectory .\.artifacts\hardware-center\x64
```

再使用企业发布证书签名普通二进制和 PowerShell 工具，并绑定清单：

```powershell
.\scripts\windows\sign-release-package.ps1 `
  -PackageRoot .\.artifacts\windows-release\0.1.0\x64 `
  -CertificateThumbprint <固定的发布证书指纹> `
  -TimestampServer https://<组织批准的-rfc3161-时间戳服务>
```

签名脚本会用 `/kp /c` 验证 INF/SYS 的生产内核 Catalog，不会把普通企业签名
冒充 Microsoft 内核信任。发布目录一旦生成 `release-trust.ps1` 就拒绝原地重签；
任何变更都必须重新组装新目录。

独立校验：

```powershell
.\scripts\windows\verify-release-package.ps1 `
  -PackageRoot .\.artifacts\windows-release\0.1.0\x64 `
  -ExpectedPublisherThumbprint <由受控渠道获得的固定指纹>
```

固定指纹必须来自构建配置、企业发布清单或另一条受控渠道，不能复制包内提示值
作为唯一信任依据。

这些 PowerShell 命令属于装有 SDK/WDK 的工程验收环境。Microsoft 不允许把
`signtool.exe` 当普通产品依赖重新分发；消费级安装 bootstrap 必须改用
`WinVerifyTrust`/Catalog Admin API 做等价验证。该 bootstrap 尚未落地，所以
当前 Windows 系统组件安装 UI 保持“不可用”是有意的安全门，而不是遗漏一个启动 PowerShell
的按钮。

## 4. 系统生命周期验收

只读查询不要求管理员或系统变更开关，但仍要求一个可信发布包：

```powershell
.\scripts\windows\system-lifecycle-e2e.ps1 Query `
  -PackageRoot <发布目录> `
  -ExpectedPublisherThumbprint <固定指纹> `
  -EvidenceDirectory C:\NonProxyEvidence\query-001
```

安装、修复和卸载必须在提升后的 PowerShell 中显式开启：

```powershell
$env:NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION = "1"
.\scripts\windows\system-lifecycle-e2e.ps1 Install `
  -PackageRoot <发布目录> `
  -ExpectedPublisherThumbprint <固定指纹> `
  -EvidenceDirectory C:\NonProxyEvidence\install-001 `
  -ExitProbeEndpoint https://probe.example/v1/exit `
  -ExitProbePublicKeys <old-public-key>,<new-public-key> `
  -ConfirmSystemMutation
Remove-Item Env:\NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION
```

出口探针参数只能用于 `Install`/`Repair`，endpoint 必须是无凭据、query 和 fragment
的 HTTPS 地址，公钥集合必须包含 1～4 把不重复的 43 位 base64url 公钥。两个参数
都不传时保留当前 Service 配置，不能用漏传参数意外清空信任；只传一项会失败。
查询证据包含 endpoint、是否已配置和受信公钥数量，但不复制公钥或任何私钥。
升级失败自动恢复旧二进制时也恢复旧 endpoint/公钥集合。

分别以新的空证据目录执行 `Install`、同版本 `Repair`、跨版本升级、失败注入回滚
和 `Uninstall`。工具不会覆盖已有证据目录，也不会自动重启。

必须人工核对：

- `NonProxyWfp` 已进入 Driver Store，服务依赖 BFE；
- `NonProxyGateway` 为 LocalSystem、Automatic，并依赖 BFE/Driver；
- Service ImagePath 位于 `%ProgramFiles%\NonProxy\system\...`；
- 状态目录不向其他普通用户开放；
- 安装用户可读取 `session.capability`，但不能修改；
- Service 停止或控制句柄退出后，动态 WFP 对象被清理；
- 新版本启动失败时旧版本恢复 Running；
- 卸载后网络恢复，用户规则默认仍在 `%ProgramData%\NonProxy`；
- 需要重启时 UI/工具只提示，不强制重启。

Windows adapter-host 纳入发布生命周期后还必须单独核对：进程以当前交互用户而非
LocalSystem 运行；状态目录位于该用户 LocalAppData；`adapter.capability` 仅该用户可读写；
`NonProxy.Adapter.v1` 拒绝远端和其他普通用户；桌面端携带错误/过期能力令牌时 RPC 被拒绝；
退出、登录切换、升级和卸载不会留下可被后续用户复用的管道或令牌。当前发布工具尚未分发或
启动该进程，因此这些仍是未完成的 W2/W3 验收项。

## 5. Driver Verifier

先创建 VM 快照并准备离线恢复。启用前确认被测 Driver 已安装：

```powershell
$env:NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION = "1"
.\scripts\windows\driver-verifier.ps1 Enable `
  -ConfirmSystemMutation `
  -AcknowledgeTestMachineOnly
Remove-Item Env:\NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION
```

由操作者安排重启，完成高 TCP/UDP/QUIC 负载、休眠唤醒、Service 强制终止、
Driver 禁用/启用和网络切换，再保存：

- `verifier /querysettings` 与 `verifier /query`；
- Kernel dump、BugCheck code 和符号化栈；
- NonProxy Driver 队列/drop/injection 计数；
- WFP state、系统事件和测试时间线。

全局重置会清除机器上所有 Driver Verifier 设置，必须使用额外开关：

```powershell
$env:NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION = "1"
$env:NONPROXY_CONFIRM_VERIFIER_GLOBAL_RESET = "1"
.\scripts\windows\driver-verifier.ps1 Reset `
  -ConfirmSystemMutation `
  -AcknowledgeTestMachineOnly `
  -ConfirmResetAllVerifierSettings
Remove-Item Env:\NONPROXY_CONFIRM_VERIFIER_GLOBAL_RESET
Remove-Item Env:\NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION
```

## 6. 真实 VPN 网络矩阵

至少覆盖：

| 维度 | 必测值 |
|---|---|
| OS/架构 | Windows 10 22H2 x64、Windows 11 x64、Windows 11 ARM64 |
| VPN 数据面 | Wintun/WireGuard、OpenVPN/TAP、至少一个封闭商业客户端 |
| VPN 状态 | 启动前已连接、运行中连接/断开、重连、服务器切换 |
| 网络 | Wi-Fi、有线、双栈、仅 IPv4；有条件时仅 IPv6/NAT64 |
| 生命周期 | 冷启动、重启、睡眠唤醒、网卡切换、Service 崩溃 |

每个 VPN 组合至少执行：

1. 在应用直连页分别选择一个运行中的 Authenticode 应用、一个通过 `.exe` 选择器加入的
   应用和一个 MSIX/UWP AppContainer 应用；确认 Win32 signer、包 PublisherId/package SID
   与运行时活动记录一致。再以默认 PROXY、指定应用 DIRECT 访问事先未知的 TCP/UDP 目标，
   并确认三类规则分别命中。
2. 浏览器一个标签页网站 DIRECT，同时另一标签页保持 PROXY。
3. DIRECT/PROXY 各自的 IPv4、IPv6、TCP、DNS UDP/TCP、connected UDP、
   `sendto`、空 UDP 与 QUIC。
4. 代理断开时 PROXY fail-closed，DIRECT 保持物理可达。
5. 物理接口消失时 DIRECT 明确失败，不能静默回落到 VPN。
6. DNS 探针失败时普通 TCP 捕获保持安全退化；DNS 与通用 UDP 不重复捕获。
7. 队列饱和、畸形 context、未知 App ID、非空畸形/错配 package SID 和进程退出时执行
   文档化失败语义；包 SID 不得降级为 Win32 身份。
8. 使用无签名副本、同名不同路径、同路径不同 signer、不同 PublisherId 的同名包、包升级、
   快速退出重启和 PID 复用验证：无签名/错 signer/错 PublisherId 不得命中，合法更新仍应
   命中对应 ALE App ID 或 versionless package family SID。

### 单条用例必须留存的四层证据

```text
配置：策略版本、规则 ID、App/域名选择条件
决策：DIRECT/PROXY/BLOCK、failure mode、命中优先级
路径：本地/远端元组、物理接口 LUID/index 或代理 outbound、DNS 路径
出口：自有探针观察到的公网 IP/地址族/时间
```

还需保存同一时间窗的 ETW/WFP state 和物理/VPN 接口抓包。验收中必须证明
DIRECT 包出现在物理接口且不先进入第三方 VPN；仅比较公网 IP 不足以排除 VPN
回送、NAT 或同出口误判。

### 判失败

- DIRECT 与 PROXY 出口相同但没有额外路径证据；
- 只看到规则已保存或 Service Running；
- TCP 通过但 UDP/QUIC、IPv6 或 DNS 未测；
- 依靠禁用 Secure Boot、测试签名模式或临时证书；
- Driver Verifier 有未解释的告警、BugCheck 或资源泄漏；
- 升级失败后需要手工恢复网络；
- 证据包含明文凭据，或时间线无法关联策略版本。

## 7. 当前仓库证据边界

仓库已经提供发布目录、签名校验、Primitive Driver 安装/卸载、版本化复制、
失败回滚、生命周期留证、Driver Verifier 安全门和 CI PowerShell 解析入口。

当前开发环境不是 Windows，尚未执行原生 Win32/打包应用目录、WinTrust、PFN/package SID、
PublisherId 与 WFP 身份联合验收、WDK C 构建、Hardware Dev Center 签名、SCM/UAC、
Driver Verifier 或真实 VPN W4 验收。
Avalonia Windows 宿主因此继续
把系统组件显示为不可用；在签名 bootstrap 和上述 W2～W4 证据完成前不得改成
“可安装”或“已确认直连”。

MSIX/UWP 包目录、ALE package SID 与 PublisherId 已有 W0 源码和交叉编译证据，但没有真实
Windows 验收结果；不得用文件名、展示名、普通路径、桌面目录成功或单元测试替代包身份证据。

Adapter 命名管道服务端、桌面客户端、独立能力文件路径和平台能力降级已有 W0 源码与
x64/ARM64 交叉编译门禁；按用户分发/启动、生产 SDDL、真实命名管道 RPC 和第三方客户端
重载尚未在 Windows 执行，不得据此声明“客户端协同可用”。

## 8. 官方依据

- [Creating a primitive driver](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/creating-a-primitive-driver)
- [INF DefaultInstall Section](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/inf-defaultinstall-section)
- [DiUninstallDriverW](https://learn.microsoft.com/en-us/windows/win32/api/newdev/nf-newdev-diuninstalldriverw)
- [Verifying the release signature](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/verifying-the-release-signature)
- [Using Inf2Cat to create a catalog](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/using-inf2cat-to-create-a-catalog-file)
- [Driver Verifier](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/driver-verifier)
- [PnPUtil examples](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/pnputil-examples)
