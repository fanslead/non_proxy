# ADR-0034：将 Windows 打包应用绑定到 ALE package SID

- 状态：已接受，待真实 Windows 设备验收
- 日期：2026-08-01
- 决策范围：MSIX/UWP 应用目录、WFP ABI、运行时身份与规则匹配

## 背景

MSIX/UWP 流量不能可靠地用安装目录、进程文件名或展示名识别。WFP ALE 层为
AppContainer 流提供 `ALE_PACKAGE_ID`，其值是 application package SID；Windows 包目录
则提供 Package Family Name（PFN）和 PublisherId。只有桌面选择与 TCP/UDP 数据面使用同一
SID，并在运行时重新闭合 PFN、SID 和发布者，带 signer 的规则才会真实命中且不接受同名冒充。

## 决策

1. Windows 生产宿主使用 `net10.0-windows10.0.26100.0` 的官方 Windows SDK projection，
   通过 `PackageManager.FindPackagesForUser("")` 枚举当前用户包。过滤 framework、resource、
   bundle、不可用包和没有 AppListEntry 的包；目录失败只标记打包应用不可用，不隐藏 Win32。
2. 包候选保存 PFN 和 `Package.Id.PublisherId`。保存规则前调用
   `DeriveAppContainerSidFromAppContainerName(PFN)`，只接受 revision 1、authority 15、八个
   subauthority 且首个 RID 为 2 的 40 字节 application package SID。稳定身份固定为
   `package-sid:S-1-15-2-...`。
3. 包 signer 固定为 `package-publisher-id:<lowercase PublisherId>`。PublisherId 必须是 13 个
   ASCII 字母或数字；目录值不满足约束时不创建规则。它是 OS 包身份的一部分，不等同于对任意
   `.exe` 执行 Authenticode 校验。
4. WFP redirect context 和 UDP datagram ABI 升级为 v2。Driver 从 TCP connect redirect 与
   UDP flow-established 的 `ALE_PACKAGE_ID` 读取有效 SID，限制为 68 字节，并按
   TCP `App ID || package SID`、UDP `App ID || package SID || payload` 的版本化长度字段传给
   gateway。内核不解析 PFN、不执行产品策略。
5. 非空 package SID 优先于 Win32 App ID。gateway 严格解码 SID，再按 PID 打开当前进程，
   调用 `GetPackageFamilyName`，从 PFN 重新派生 SID 并与 WFP 原始字节等值核对，随后用
   `PackageNameAndPublisherIdFromFamilyName` 解析 PublisherId。只有进程创建时间可读且整个
   闭环成立时才附加 signer。
6. 非空但畸形、过长、与进程 PFN 不一致或发布者不可解析的 package SID 不得降级为 Win32
   身份。畸形 SID 生成 `unknown-app`；合法但无法完成运行时闭环时保留 package stable ID、
   省略 signer，使带 signer 规则安全地不命中。
7. 没有 ALE package SID 的流继续使用 ADR-0033 的 Win32 App ID + Authenticode 链。部分
   full-trust packaged process 可能落入此分支；产品只声明实际 WFP 元数据所能证明的身份，
   不根据安装路径猜包归属。
8. 两类 Windows 应用规则都保持 `include_helpers=false`。一个包可能包含多个 AppListEntry，
   当前规则有意按 PFN/package SID 覆盖同一 package family，而不是虚构跨包 Helper 关系。
9. portable `net10.0` 目标只为非 Windows 单元测试保留，并返回“包目录不可用”；MSBuild 明确
   拒绝用该目标发布，避免生产包静默丢失 package projection。

## 失败语义

- 单个坏包被跳过；PackageManager/WinRT 整体失败时 Win32 目录仍可用并显示降级提示。
- Driver 看到类型错误、无效或过长 SID 时，TCP context 创建失败并沿现有有计数 fail-open
  处理；通用 UDP 无 flow context 时沿现有 fail-closed 处理。
- 用户态身份解析受现有 32 并发上限和 4096 项、按 PID/创建时间/稳定身份分代的有界缓存
  约束；容量不足时不附加 signer。
- 目录成功、规则保存和 Provider ACK 都不能替代真实 WFP 命中及物理出口证据。

## 验证边界

仓库测试覆盖 C#/Rust 同一 package SID 与 PublisherId 向量、畸形非空 SID 不降级、SID/PFN
错配不附加 signer、WFP ABI v2 长度拆分、x64 Windows 严格 Clippy 和 ARM64 交叉检查。
macOS 上的双 TFM 编译只能证明 Windows SDK projection 可编译。WDK C 构建、当前用户包目录
权限、AppContainer 与 full-trust packaged process 的真实元数据、PID 复用、TCP/UDP 命中和
VPN 路径仍按 Windows 系统验收手册取证。

## 官方依据

- [FWPS_FIELDS_ALE_CONNECT_REDIRECT_V4: ALE_PACKAGE_ID](https://learn.microsoft.com/en-us/windows/win32/api/fwpsu/ne-fwpsu-fwps_fields_ale_connect_redirect_v4)
- [FWPS_FIELDS_ALE_FLOW_ESTABLISHED_V4: ALE_PACKAGE_ID](https://learn.microsoft.com/en-us/windows/win32/api/fwpsu/ne-fwpsu-fwps_fields_ale_flow_established_v4)
- [PackageManager.FindPackagesForUser](https://learn.microsoft.com/en-us/uwp/api/windows.management.deployment.packagemanager.findpackagesforuser)
- [PackageId.FamilyName](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.packageid.familyname)
- [PackageId.PublisherId](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.packageid.publisherid)
- [DeriveAppContainerSidFromAppContainerName](https://learn.microsoft.com/en-us/windows/win32/api/userenv/nf-userenv-deriveappcontainersidfromappcontainername)
- [GetPackageFamilyName](https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-getpackagefamilyname)
- [PackageNameAndPublisherIdFromFamilyName](https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-packagenameandpublisheridfromfamilyname)
