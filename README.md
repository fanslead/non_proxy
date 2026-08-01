# NonProxy

NonProxy 是一个本地优先的跨平台智能分流网关。用户选择应用或网站后，系统把匹配流量明确路由为 `DIRECT` 或 `PROXY`，并保留决策、路径和出口证据。

当前状态：按 `docs/TECHNICAL_IMPLEMENTATION.md` 实施中。仓库内的构建通过不代表 macOS System Extension、Windows WFP 或真实网络路径已经验收。

共享基础已经覆盖版本化契约、Rust 策略/编译器/SQLite 权威存储、认证本地控制面、Avalonia 工作区、SOCKS5/HTTP CONNECT 出口、系统凭据库和有背压的 NPF1 TCP/UDP 数据通道。网络出口页既能只读发现 macOS 当前公开的系统 SOCKS/HTTP 代理，也支持逐行粘贴标准代理链接；两条入口都先显示不含账号密码的识别预览，再由用户明确批量保存，凭据只进入系统凭据库。用户可在共享桌面端选择默认代理；只有当前配置在 60 秒内通过代理握手后，服务端才允许把它设为默认。默认路由配置、乐观 revision 与待确认快照在同一事务发布，未命中规则的流量使用快照中的 fail-closed 默认代理，回滚也会恢复历史默认路由。独立出口探针已经具备 HTTPS + Ed25519 签名回执、最多四把公钥的零停机轮换、固定安装信任、DIRECT/PROXY 网关编排、Provider EXIT 越级拒绝、不可变有界回执存储、共享桌面端触发/展示、Linux 生产部署与安全密钥管理工具；真实公网双路径仍需在正式环境验收。浏览器学习链路包含 Safari/Chromium 正确入口、标签页隔离、临时站点权限、候选审核和幂等规则确认；macOS 已具备真实 Transparent/DNS Provider、物理 DIRECT、代理出口、同时绑定固定代码签名标识与正式 TeamIdentifier 的 gatewayd 防回环系统规则、System Extension Bundle、LaunchAgent 生命周期与打包冒烟；旧快照会在 Provider 启动前原子升级，回滚也会重建当前系统规则，当前保护快照激活前 gatewayd 不会建立代理上游连接。临时签名只能提供不带 TeamIdentifier 的开发身份，交叉构建和夹具回显也只属于仓库证据；正式签名、系统授权与真实 VPN 共存仍需独立验收。

Windows 已接入 SCM Service、受限命名管道、共享 UI、最小 WFP ALE Connect Redirect Driver、动态 BFE session、版本化 IOCTL/context ABI、redirect records 和用户态 TCP DIRECT/PROXY/BLOCK。DIRECT TCP/DNS 会实时选择可信物理接口并设置 `IP_UNICAST_IF`/`IPV6_UNICAST_IF`；没有物理路径时明确失败。域名身份运行时会在确认 `198.18.0.0/15` 无路由冲突后分配可恢复的 IPv4/安装级 ULA 合成地址，由 WFP TCP 代理反查域名并重新物理解域或保留给远端代理。

Windows DNS 不修改网卡设置：动态 WFP filter 先只把远端 TCP/UDP 53 重定向到随机 loopback listener，系统 resolver 探针通过且活动策略就绪后才启用普通 TCP；探针失效则退回 DNS-only。远端 53 之外的 UDP/QUIC 使用同一最小 Driver 的 ALE flow 身份关联、DATAGRAM_DATA 有界搬运和入站重注入，Service 继续执行 App/域名策略、物理 DIRECT 或 SOCKS5 UDP。

Windows 发行层已经加入 Primitive Driver INF、固定发布者 + 已签名清单绑定、复制后复验、版本化 Service/Driver 安装、失败回滚、默认保留数据的卸载、Driver Verifier 安全门与生命周期证据清单。当前 Rust 单测和 Windows 交叉门禁覆盖用户态与 ABI；WDK 实机结果、Hardware Dev Center 生产签名、SCM/UAC、Driver Verifier 与真实 VPN 路径尚未验收，因此 Windows UI 仍不会把系统组件表述为可用。

运行概览会按“后台服务 → 透明代理 → DNS 分流 → 网络接管”显示真实分段状态。等待授权时可通过原生桥直接打开 macOS“登录项与扩展”，允许后重新检查；部分安装可执行修复，卸载需要二次确认。诊断页复用同一组分段证据并显示稳定错误码，还可以生成最近 24 小时的严格脱敏本地 JSON；页面会预览内容范围、大小和 SHA-256，文件不包含凭据、代理端点、网络载荷或逐连接样本，也不会自动上传。

共享桌面端已经加入当前网络一键直连：macOS 只在用户点击时采集当前物理网络并在宿主进程内生成隐私安全指纹，原始 SSID 不跨越原生边界；网络档案、网络作用域规则和待确认快照按 revision 编排，明确区分“已保存”“等待确认”和“已激活”。桌面生命周期使用跨平台托盘与原生菜单，关闭窗口只隐藏界面，恢复窗口和“只退出界面”是不同动作；系统/浅色/深色主题采用有界、原子、本地设置。控制事件作为页面失效信号，侧栏会显示订阅连接状态，中断重连后重新读取权威 RPC 快照，不把事件摘要当成本地事实。智能学习的实际入口位于当前浏览器标签页，桌面页不再提供无法关联标签页的伪开始按钮。

活动记录现在保存并展示认证 Provider 解析出的应用签名、父应用与 Helper 归属。只有当前平台
且签名身份完整的非系统记录才提供“让此应用始终直连”，并在明确确认后创建带 signer 约束
的应用规则；macOS 可包含已经认证的辅助进程，Windows 当前只匹配精确 Win32 可执行文件。
旧记录、身份不足、跨平台和已有规则都会保持只读说明。规则保存后
继续等待 Provider ACK，页面不会把 pending 误报成已经生效。

运行概览现在持续核对“完整网关”和“客户端协同”两条接入路径，不保存会过期的向导完成状态。完整网关只有在系统组件、默认代理、直连规则和活动数据面同时满足时显示基础就绪；客户端登记仍保持待同步/待路径证据。引导按钮只切换到既有权威页面，不会根据未知的 MDM、Always-On VPN 或第三方 TUN 猜测环境已经兼容。

“全部规则”页可以二次确认后恢复上一份真正生效的配置。恢复点由后台从有效快照历史选取，操作同时绑定当前活动版本并原子恢复当时的默认路由；并发发布会拒绝旧确认，Provider ACK 前只显示等待确认。当前规则草稿不会被回滚操作删除。

第三方客户端协同已覆盖 Surge、Clash/Mihomo 和 sing-box 的显式登记、活动快照无损投影、客户端原生校验、主配置与 sidecar 双文件事务、公开重载、配置确认和失败恢复。共享页面可用系统原生文件选择器选取客户端与当前配置，也保留绝对路径高级回退；所有选择结果仍由隔离 adapter-host 重新验证。页面独立显示候选校验、配置载入和真实路径，当前没有路径级证据时始终提示“尚未证明绕过 VPN”。Windows 源码已接入独立的受限 Adapter 命名管道、会话能力文件和共享页面客户端；Surge 在非 macOS 平台失败关闭，sing-box 在 Windows 不会误报尚无安全实现的热重载，Mihomo 保留 loopback HTTP 重载能力。Windows 按用户打包、启动、升级和 ACL 实机验收尚未完成，因此这条源码链路不能表述为产品可用。

应用直连页已经在 macOS 与 Windows 共用。Windows 目录从当前用户运行进程和系统注册应用中筛选可信 Win32 `.exe`，也可通过系统文件选择器补充；稳定身份直接来自 `FwpmGetAppIdFromFileName0`，只有通过 Authenticode 信任校验并取得证书 SHA-256 的应用才能创建规则。MSIX/UWP 由当前用户包目录提供 PFN 与 PublisherId，规则稳定身份使用系统派生的 AppContainer package SID。WFP TCP/UDP 会携带 ALE package SID，并按 PID 重新读取 PFN、派生 SID、解析 PublisherId 后等值核对；非空畸形包 SID 不回退为 Win32。Windows 不虚构 Helper 关系；真实 Windows 上的包目录、WinTrust/Catalog 签名、PID 复用与 WFP 命中仍按验收文档取证。

正式签名 macOS 包的只读查询、安装、升级、卸载和完整生命周期验收使用 [macOS 系统组件验收手册](docs/MACOS_SYSTEM_ACCEPTANCE.md)。验收命令拒绝临时签名、非 `/Applications` 包和未经显式确认的系统变更，并输出带 SHA-256 清单的独立证据目录。

Safari 扩展的正式登记、启用、普通/无痕窗口与多标签页验收使用 [Safari Web Extension 正式验收](docs/SAFARI_EXTENSION_ACCEPTANCE.md)。

Windows 的构建签名、安装/修复/升级/卸载、Driver Verifier 和真实 VPN
TCP/DNS/UDP/QUIC 验收使用
[Windows 系统组件与真实网络路径验收](docs/WINDOWS_SYSTEM_ACCEPTANCE.md)。

## 架构

- `apps/desktop/`：唯一的 Avalonia 跨平台桌面 UI。
- `crates/`：Rust 领域模型、策略、DNS、存储和安全核心。
- `services/`：`gatewayd`、适配器宿主和测试探针。
- `platform/`：macOS Network/System Extension 与 Windows WFP。
- `packages/`：浏览器扩展和 TypeScript 共享包。
- `proto/`：跨进程契约唯一来源。
- `generated/`：生成代码，禁止手工编辑。

完整边界和交付顺序见：

- [产品方案](NONPROXY_PRODUCT_SOLUTION.md)
- [技术实现](docs/TECHNICAL_IMPLEMENTATION.md)
- [桌面 UI ADR](docs/ADR/0001-use-avalonia-for-cross-platform-desktop-ui.md)
- [macOS Provider 策略运行时 ADR](docs/ADR/0002-use-native-provider-policy-runtime.md)
- [macOS 最低版本 ADR](docs/ADR/0003-set-macos-15-minimum.md)
- [Windows 最小 WFP Callout ADR](docs/ADR/0004-use-minimal-wfp-connect-redirect-callout.md)
- [Windows DIRECT 物理接口 ADR](docs/ADR/0005-bind-windows-direct-to-physical-interface.md)
- [Windows 选择性合成 DNS ADR](docs/ADR/0006-use-selective-synthetic-dns-on-windows.md)
- [Windows WFP 明文 DNS 截获 ADR](docs/ADR/0007-intercept-windows-dns-with-wfp.md)
- [Windows UDP/QUIC 数据报搬运 ADR](docs/ADR/0008-divert-windows-udp-datagrams.md)
- [Windows 系统组件发行 ADR](docs/ADR/0009-distribute-windows-system-components.md)
- [脱敏诊断包 ADR](docs/ADR/0010-export-redacted-diagnostics-from-gateway.md)
- [标准代理链接导入 ADR](docs/ADR/0011-import-standard-proxy-uris.md)
- [系统代理自动发现 ADR](docs/ADR/0012-discover-public-system-proxy-settings.md)
- [默认代理握手门禁 ADR](docs/ADR/0013-require-fresh-handshake-before-default-route.md)
- [网络档案快照绑定 ADR](docs/ADR/0014-bind-network-profiles-to-signed-snapshots.md)
- [macOS 当前网络身份 ADR](docs/ADR/0015-resolve-macos-network-environment-at-runtime.md)
- [当前网络一键直连 ADR](docs/ADR/0016-orchestrate-one-click-network-direct.md)
- [桌面生命周期与真实设置 ADR](docs/ADR/0017-own-desktop-lifetime-and-truthful-local-settings.md)
- [桌面控制事件失效信号 ADR](docs/ADR/0018-use-control-events-as-invalidation-signals.md)
- [第三方客户端规则格式 ADR](docs/ADR/0019-generate-versioned-client-rule-sets.md)
- [可恢复 Adapter 文件事务 ADR](docs/ADR/0020-use-recoverable-adapter-file-transactions.md)
- [认证 Adapter Host ADR](docs/ADR/0021-run-adapters-in-an-authenticated-host.md)
- [客户端原生候选校验 ADR](docs/ADR/0022-validate-adapter-candidates-before-persistence.md)
- [第三方主配置无损集成 ADR](docs/ADR/0023-patch-adapter-main-configurations-losslessly.md)
- [sidecar 与主配置事务 ADR](docs/ADR/0024-coordinate-sidecar-and-main-configuration-transactions.md)
- [Adapter RPC 配置绑定 ADR](docs/ADR/0025-bind-integrated-configurations-to-adapter-rpc.md)
- [公开客户端重载 ADR](docs/ADR/0026-reload-adapter-clients-with-public-controls.md)
- [活动快照 Adapter 投影 ADR](docs/ADR/0027-project-adapter-rules-from-active-snapshots.md)
- [桌面 Adapter 同步编排 ADR](docs/ADR/0028-orchestrate-adapter-sync-from-the-desktop.md)
- [权威接入就绪引导 ADR](docs/ADR/0029-guide-setup-with-authoritative-readiness.md)
- [上一有效快照恢复 ADR](docs/ADR/0030-restore-the-previous-effective-snapshot.md)
- [有时限运行态路由覆盖 ADR](docs/ADR/0031-time-bounded-runtime-routing-overrides.md)
- [活动证据创建签名应用规则 ADR](docs/ADR/0032-create-signed-app-rules-from-activity.md)
- [Windows WFP 与 Authenticode 应用身份 ADR](docs/ADR/0033-bind-windows-app-rules-to-wfp-and-authenticode.md)
- [Windows 打包应用 ALE 身份 ADR](docs/ADR/0034-bind-packaged-apps-to-ale-package-sid.md)
- [Windows Adapter 命名管道 ADR](docs/ADR/0035-connect-windows-adapter-host-over-named-pipes.md)
- [AI/工程协作规则](AGENTS.md)

## 本地工具链

macOS 开发机可以把固定版本工具安装到仓库自己的 `.tools/`，不会改动系统默认工具链：

```bash
./scripts/bootstrap/install-local-tools.sh
source ./scripts/bootstrap/env.sh
./scripts/bootstrap/check-prerequisites.sh
```

已有符合版本要求的全局 .NET、Node.js 和 pnpm 会直接复用。Rust、Buf、Protobuf Compiler 和 just 使用校验过 SHA-256 的固定版本。

## 常用命令

```bash
source ./scripts/bootstrap/env.sh
just check-tools
just generate
just contracts
just format-check
just lint
just test
just native-bridge-smoke
just gateway-bundle-smoke
just check
```

只有当前已经建立的语言工作区会运行。随着各组件落地，根任务保持同一入口并逐步收紧门禁。

契约基线提交后，可以在修改 `.proto` 文件时运行：

```bash
just contracts-breaking
```

## 平台验收边界

- macOS Provider 必须在真实 System Extension 环境验证。
- Windows WFP/Driver 必须在真实 Windows/WDK 环境验证。
- `DIRECT`/`PROXY` 的完成声明需要路径证据，不能只使用配置或单元测试。
- 签名、Notarization、Driver signing 和独立机器安装属于发布门禁。
