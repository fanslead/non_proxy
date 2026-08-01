# ADR-0009：以固定发布者和版本化系统目录交付 Windows 组件

- 状态：已接受，待真实 Windows 发行验收
- 日期：2026-07-31

## 背景

NonProxy 的 Windows 数据面包含普通用户桌面应用、普通用户 adapter-host、LocalSystem
Service 和 WFP Callout Driver。仅把 `SYS/INF/EXE` 放进压缩包会留下四类风险：

- 包内 JSON 可以被同时替换，不能自行充当信任根；
- 用户可写目录中的 Service 二进制容易产生提权和 TOCTOU 风险；
- Primitive Driver 的安装、卸载、重启和升级不是普通文件复制；
- 新 Service 或 Driver 启动失败时可能让流量捕获停在部分安装状态。

Windows 10 1903 起的软件组件应按 Primitive Driver 规则使用带架构修饰的
`DefaultInstall`。驱动文件进入 Driver Store，运行副本使用 DIRID 13。安装和
卸载分别使用 `DiInstallDriverW` 与 `DiUninstallDriverW`，不使用设备实例或伪造
Hardware ID。

## 决策

### 信任

1. 桌面应用入口、Service、安装工具和 PowerShell 工具使用同一企业代码签名
   发布者并带 RFC 3161 时间戳。self-contained 运行库保留原厂签名，由已签名
   清单绑定哈希，不用项目证书覆盖 Microsoft 等上游签名。
2. WFP Catalog 必须先取得 Microsoft Hardware Dev Center 的 Attestation 或
   HLK 签名；普通企业 Authenticode 不能替代生产内核签名。
3. `release-manifest.json` 记录完整相对路径、大小和 SHA-256。
4. `release-trust.ps1` 只保存清单 SHA-256，并由发布者 Authenticode 签名。
   校验器不执行该文件，只解析固定赋值文本。
5. 信任根由调用方传入的固定发布者指纹决定。包内的指纹只允许作为显示提示，
   不能决定自身是否可信。
6. 校验必须拒绝清单外文件、重复/逃逸路径、重解析点、错误架构和过低系统版本；
   INF/SYS 还必须通过 `signtool verify /kp /c` 的 Catalog 成员校验。

### 安装

1. 所有系统变更都要求管理员权限、`-ConfirmSystemMutation` 和
   `NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION=1` 同时存在。
2. 先校验下载目录，再复制完整包到
   `%ProgramFiles%\NonProxy\system\<version>-<arch>-<instance>`，然后在管理员
   目录中再次校验，避免直接运行用户可写目录中的 Service。
3. Driver 通过 `DiInstallDriverW(..., Flags=0)` 安装。Service 名固定为
   `NonProxyGateway`，以 LocalSystem 自动启动，依赖 `BFE` 和 `NonProxyWfp`。
4. Service 的状态目录固定为 `%ProgramData%\NonProxy`。ACL 只给予 SYSTEM、
   Administrators 和执行安装的用户，不给其他交互用户能力文件读取权。
5. Service 环境明确下发状态目录、两条命名管道、生产 SDDL 和网关二进制
   SHA-256。Service 模式缺任一生产安全值时拒绝启动。
6. 每次安装使用新版本目录。新组件启动失败时重新安装上一份 INF、恢复上一份
   Service 路径/环境和注册表元数据。无法自动回滚时保留文件并报告人工恢复。
7. `DiInstallDriverW` 或 `DiUninstallDriverW` 返回需要重启时只报告状态，工具
   不自动重启。
8. 默认卸载保留 `%ProgramData%\NonProxy`。清除用户数据需要第二组显式参数
   和环境开关。
9. adapter-host 位于同一管理员保护的版本目录并进入固定发布者签名清单。机器级登录任务
   使用内置 Users group、空用户登录触发器、受限 token 和并行实例；桌面端复验安装注册表、
   路径、重解析点与 SHA-256 后在当前真实用户会话补齐首次启动。卸载先撤销任务，再精确终止
   受保护版本目录中的计划任务/桌面补启动实例，最后删除版本载荷。

### Driver Verifier

Driver Verifier 只允许在有快照、可离线恢复的测试机启用。工具只针对
`NonProxyWfp.sys` 启用标准规则，不自动重启。由于 `verifier /reset` 会影响整台
机器，重置还需要独立的全局确认开关。

## 结果

- 发布包可以离线校验且不会信任自身声明的发布者。
- 安装、修复、升级回滚和卸载具有同一条可审计入口。
- x64 与 ARM64 包必须分别构建、签名和验收。
- PowerShell 目前是工程发行工具，不是最终消费级安装体验。Avalonia UI 只有在
  固定发布者的签名 bootstrap 完成并通过真实 Windows UAC/SCM 验收后，才能把
  Windows 系统组件从“不可用”改为可安装。
- `signtool.exe` 是 Windows SDK 工程工具，不随产品分发。最终签名 bootstrap
  必须直接使用 `WinVerifyTrust` 与 Catalog Admin API 完成等价校验，并用包外
  编译固定的发布者身份；不得把 SignTool 复制进消费级安装包。
- 当前 macOS 开发机不能证明 WDK、生产签名、SCM、重启回滚或第三方 VPN filter
  顺序；这些结果必须按 Windows 验收手册单独取得。

## 参考

- [Microsoft：Creating a primitive driver](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/creating-a-primitive-driver)
- [Microsoft：INF DefaultInstall Section](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/inf-defaultinstall-section)
- [Microsoft：DiInstallDriverW](https://learn.microsoft.com/en-us/windows/win32/api/newdev/nf-newdev-diinstalldriverw)
- [Microsoft：DiUninstallDriverW](https://learn.microsoft.com/en-us/windows/win32/api/newdev/nf-newdev-diuninstalldriverw)
- [Microsoft：Verifying the release signature](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/verifying-the-release-signature)
- [Microsoft：Installing a catalog file by using SignTool](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/installing-a-catalog-file-by-using-signtool)
- [Microsoft：Driver Verifier](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/driver-verifier)
