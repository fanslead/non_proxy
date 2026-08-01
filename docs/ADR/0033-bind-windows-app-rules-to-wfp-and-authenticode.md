# ADR-0033：将 Windows 应用规则绑定到 WFP 身份与 Authenticode 发布者

- 状态：已接受，待真实 Windows 设备验收
- 日期：2026-08-01
- 决策范围：Windows Win32 应用目录、应用规则身份和运行时匹配

## 背景

WFP ALE 连接元数据中的应用身份不是普通的 `C:\...` 路径。桌面端如果自行规范化 DOS
路径，保存的规则可能永远无法命中；如果只按文件名或展示名匹配，同路径替换和同名进程又
会形成冒充边界。仅在应用目录读取 Authenticode 签名也不够，运行时身份不携带相同 signer
时，带签名约束的规则仍会稳定失配。

## 决策

1. Windows 桌面目录只从当前用户会话的 Win32 运行进程和 HKCU/HKLM `App Paths` 收集候选，
   并提供只允许选择 `.exe` 的系统文件选择器；不要求普通用户填写路径或 WFP 标识。
2. 每个候选必须调用 `FwpmGetAppIdFromFileName0`。返回的 UTF-16 `FWP_BYTE_BLOB` 是策略
   `stable_id` 的唯一来源，桌面端不把 DOS 路径、文件名或显示名重新解释为身份。
3. 候选必须通过无 UI 的 `WinVerifyTrustEx` Authenticode 策略校验。规则 signer 使用叶子
   签名证书 DER 的 SHA-256，格式固定为 `cert-sha256:<64 位小写十六进制>`；签名缺失、
   不可信或无法提取证书时不显示为可配置应用。
4. TCP/UDP 数据面根据 WFP context 的进程 ID 读取当前进程映像路径，再次调用
   `FwpmGetAppIdFromFileName0` 并与捕获的 ALE App ID 等值核对，随后执行同一
   Authenticode 校验。只有两次身份一致时才附加 signer，避免 PID 复用或路径错配把其他
   证书绑定到连接。
5. 签名结果按 `进程 ID + 进程创建时间 + ALE App ID` 有界缓存。缓存最多 4096 项；进程
   重启后必须重新验证，不能仅按长期路径缓存旧签名。阻塞式 Win32 身份解析最多并发 32
   个；容量不足时不附加 signer，不排队放大资源占用。
6. Windows 当前按精确可执行文件身份创建规则，不声明 macOS Helper/XPC 语义。
   `include_helpers=false`，活动记录确认卡也不得写“及其辅助进程”。
7. WFP ABI 最多携带 4096 字节 App ID。领域模型允许最多 8192 字节 UTF-8 稳定身份，
   桌面解码允许最多 2048 个 UTF-16 code unit；短 signer、显示名和 Helper 元数据继续
   保持 512 字节/字符级边界。
8. MSIX/UWP 使用独立 package identity 链，不复用本 ADR 的 Win32 路径/证书算法；具体
   目录、ALE package SID、PublisherId 与失败语义由 [ADR-0034](0034-bind-packaged-apps-to-ale-package-sid.md)
   约束。两类身份均不回退到文件名、展示名或未验证路径。

## 失败语义

- 目录中的单个无签名或不可读候选被跳过，不生成弱身份规则。
- 原生目录整体不可用时返回明确不可用状态，已有规则仍可查看和删除。
- 运行时无法读取进程、App ID 不一致或 Authenticode 校验失败时保留 ALE stable ID，
  但不附加 signer；因此签名约束规则安全地不命中，不回退到仅路径匹配。
- 规则保存成功仍只是待 Provider ACK；活动记录中的路径/出口证据继续独立判断。

## 验证边界

仓库测试覆盖 WFP UTF-16 解码、跨 Rust/C# 的证书哈希向量、目录去重/签名过滤、
Windows 精确 Win32 应用规则语义，以及 x64/ARM64 Windows 用户态交叉编译。它们不能替代真实
Windows 上对 `FwpmGetAppIdFromFileName0`、WinTrust provider state、Catalog-signed
二进制、进程退出/PID 复用和 WFP 规则命中的验收。

## 官方依据

- [FwpmGetAppIdFromFileName0](https://learn.microsoft.com/en-us/windows/win32/api/fwpmu/nf-fwpmu-fwpmgetappidfromfilename0)
- [WinVerifyTrust](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-winverifytrust)
- [WTHelperProvDataFromStateData](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-wthelperprovdatafromstatedata)
- [WTHelperGetProvSignerFromChain](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-wthelpergetprovsignerfromchain)
