# AGENTS.md

本文件适用于整个 NonProxy Monorepo。子目录如需更严格的平台规则，可以增加更深层的 `AGENTS.md`，但不得放宽本文件的安全、隐私和架构约束。

## 1. 开始工作前

任何 Agent 在修改代码前必须先阅读：

1. `NONPROXY_PRODUCT_SOLUTION.md`
2. `docs/TECHNICAL_IMPLEMENTATION.md`
3. 当前目标目录下更深层的 `AGENTS.md`（若存在）
4. 与任务直接相关的 ADR、协议和测试文档

先检查：

```bash
git status --short --branch
git diff --stat
```

现有未提交改动属于用户。只修改任务范围内的文件，不覆盖、不清理、不回滚无关改动。

## 2. 产品不变量

以下规则不得被普通功能需求破坏：

- 完整网关模式下，DIRECT 流量不得先进入第三方 VPN。
- PROXY 流量失败时必须执行显式 fail-closed/fail-open 策略，不能静默降级。
- 不做 TLS MITM，不安装根证书，不读取请求正文、Cookie、表单或凭据。
- 不绕过 MDM、Always-On VPN 或组织强制网络策略。
- “规则已保存”不等于“实际直连”。完成声明需要决策证据和路径证据。
- System Extension、Windows Service、Driver 和 `gatewayd` 不得信任 UI、浏览器扩展或外部订阅输入。
- 主应用退出后，数据面必须继续使用最后一份已验证策略。
- 配置更新必须原子化并保留回滚点。
- 密钥、Token、私钥、订阅密钥和密码不得进入日志、fixture 或普通数据库字段。

## 3. Monorepo 边界

计划中的顶层职责：

- `apps/desktop/`：唯一的 Avalonia 跨平台桌面 UI。
- `platform/`：Network Extension、WFP、Driver、安装和平台 API。
- `services/`：`gatewayd`、适配器宿主和测试探针等可执行服务。
- `crates/`：共享 Rust 领域模型、策略、DNS、存储、协议接口和安全核心。
- `adapters/`：第三方客户端适配器。
- `packages/`：浏览器扩展和 TypeScript 共享包。
- `proto/`：跨进程契约唯一来源。
- `generated/`：代码生成输出，禁止手工编辑。
- `migrations/`：只追加数据库迁移。
- `tests/`：跨组件、系统、性能和长期测试。
- `tools/`、`scripts/`：构建、生成、测试和发布工具。
- `docs/`：技术、ADR、安全、测试和运维文档。

不要因为某个目录尚未建立就把代码临时放进错误层。先建立正确模块。

## 4. 依赖方向

允许：

- UI -> 生成的控制客户端 -> Protobuf 契约。
- 平台捕获层 -> 共享策略核心和契约。
- `gatewayd` -> 策略、存储、协议接口、适配器接口。
- 适配器 -> `nonproxy-adapter-api`。
- 共享模块 -> `nonproxy-model`。

禁止：

- `nonproxy-model` 依赖 Avalonia、AppKit、NetworkExtension、WFP 或数据库。
- UI 直接访问 SQLite 或平台 Provider 内部状态。
- Provider/Driver 解析订阅、操作 UI 或运行复杂协议。
- Adapter 直接修改策略数据库。
- Policy Engine 发网络请求、打开数据库或记录日志副作用。
- Windows Driver 解析 JSON、Protobuf、SQLite、正则规则或外部配置。
- 跨目录复制相同领域类型来规避正确依赖。

发现依赖方向不够用时，先新增接口或 ADR，不得用反向 import、全局 singleton 或共享可变状态绕过。

## 5. 代码拆分

- 生产代码文件目标不超过 400 行。
- 超过 500 行必须拆分或在评审说明不可拆原因。
- 超过 600 行不允许，生成文件、固定数据表和测试 fixture 除外。
- Avalonia 顶层 AXAML View 目标不超过 250 行；code-behind 只处理纯 UI 生命周期。
- `main.rs`、App entry、Extension entry、Service entry 只负责组装。
- 一个文件承担一个清晰责任，但不要机械地把每个小类型拆成文件。
- 禁止创建不断膨胀的 `Utils`, `Helpers`, `Managers`, `Common`, `Misc`。
- 以领域能力命名，例如 `PolicySnapshotStore`, `AppIdentityResolver`, `TCPFlowRelay`。
- 网络、数据库、平台 API 与纯决策逻辑必须分层。

新增功能前先决定：

1. 它属于领域、应用、平台、基础设施还是 UI。
2. 是否需要跨进程契约。
3. 是否会进入高权限进程。
4. 是否需要失败和回滚路径。

## 6. Rust

- 使用 workspace 依赖和根 `Cargo.lock`。
- 共享领域代码保持平台无关。
- 公共 API 使用明确领域类型，不传递无语义 `String`/`HashMap`。
- IO 边界使用可测试 trait。
- 错误保留 source chain，并映射到稳定 `NP_<SUBSYSTEM>_<REASON>` 错误码。
- 禁止 `unwrap`/`expect` 出现在可达生产路径，除非有不可破坏的不变量并写明原因。
- 不使用全局可变状态保存策略、凭据或 flow。
- 异步任务必须有所有者、取消路径和上限。
- 队列、缓存和重试必须有界。
- 新 crate 要写清职责，不创建只有转发作用的无意义层。

验证至少包括：

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

只改一个 crate 时先跑定向测试，再跑相关 workspace 门禁。

## 7. .NET 和 Avalonia

- macOS 与 Windows 桌面 UI 统一使用 Avalonia 12 + .NET 10 LTS。
- 只在 `NonProxy.Desktop.Core` 维护一套 C#/AXAML 页面、ViewModel、主题和常规 UI 自动化。
- `NonProxy.Desktop.Mac` 和 `NonProxy.Desktop.Windows` 是薄启动宿主，不得复制页面或产品 ViewModel。
- Mac 宿主目标为 `net10.0-macos`；Windows 宿主目标为 `net10.0`。
- 使用 CommunityToolkit.Mvvm，不混用多个 MVVM 框架。
- View 只负责布局、绑定、动画和纯 UI 生命周期。
- ViewModel 不访问 SQLite、NetworkExtension、WFP、Keychain、注册表或驱动。
- ViewModel 不散布 `OperatingSystem.IsMacOS()`/`IsWindows()`；通过小型平台接口注入。
- 平台接口返回领域 DTO，不泄漏 OS handle、WFP struct 或 Apple framework 类型。
- 控制客户端使用生成的 C# Protobuf 类型，不手写重复 DTO。
- 异步命令必须支持取消、重复点击保护和错误状态。
- UI 退出不等于停止 `gatewayd` 或网络数据面。
- 发布使用 self-contained runtime；普通用户不应被要求预装 .NET。
- 首发不启用 NativeAOT，除非单独验证反射、序列化、诊断和打包链路。
- 公共页面必须在 macOS 和 Windows 都有 UI 自动化覆盖。
- macOS 14/15 当前属于 Avalonia Tier 2，相关支持声明必须有项目自己的真实设备回归证据。

验证至少包括：

```bash
dotnet format --verify-no-changes
dotnet build apps/desktop/NonProxy.Desktop.slnx -c Release --no-restore
dotnet test apps/desktop/NonProxy.Desktop.Tests -c Release --no-restore
```

## 8. Swift 和 macOS

- Swift 仅用于 Network Extension、Native Messaging Host 和必要的 macOS 平台桥接；System Extension Controller 默认位于 `net10.0-macos` 薄宿主。
- System Extension 激活/卸载请求必须从最终 containing `.app` 的 Mac 宿主提交；扩展必须嵌入 `Contents/Library/SystemExtensions/`。
- 不在 Swift 工程中复制 Avalonia 页面、ViewModel 或产品业务流程。
- Network Extension Provider 不得依赖 Avalonia。
- Provider 回调中不得同步访问磁盘、Keychain、数据库或远程网络。
- 来源应用身份使用 bundle/signing/audit identity；PID 和路径不能作为唯一长期身份。
- Provider 只使用不可变策略快照。
- DIRECT 决策在 Transparent Proxy 中交还系统；PROXY 决策进入有界 flow relay。
- TCP 必须保留 half-close 语义。
- UDP 队列必须有界。
- 所有 flow 必须有关闭、取消、超时和 backpressure 路径。
- 无法识别应用时使用明确 unknown identity，不能擅自放宽为 DIRECT。
- `gatewayd` 不可用时必须遵守缓存快照和失败策略，不得静默全部直连。
- DNS direct/proxy cache 必须分区。
- 不以 SNI 作为唯一域名来源，不通过解密 TLS 获取域名。

涉及真实 System Extension 的功能必须在真实 macOS 目标运行。仅编译或单元测试不能声明运行完成。

## 9. Windows

- UI 复用 `apps/desktop/NonProxy.Desktop.Core`，不得另建 WinUI/WPF 页面复制产品功能。
- Windows 专属代码只处理 WFP、Service、Driver、安装、签名和平台桥接。
- 应用识别基于 WFP ALE/包身份/签名，路径只作为辅助。
- 先评估用户态 WFP；只有标准功能不足时才添加 Callout Driver。
- 内核代码必须保持最小：分类、重定向和必要 metadata。
- Policy compiler、协议、订阅、数据库、遥测和复杂匹配必须留在用户态。
- 重定向连接必须正确处理 WFP redirect records/context，避免递归。
- Driver 变更必须运行 WDK 构建、签名检查和 Driver Verifier 相关测试。
- 不使用普通单元测试替代真实 Windows 网络栈验收。

## 10. Protobuf 与生成代码

- `proto/` 是跨进程契约唯一真源。
- 包名带版本，例如 `nonproxy.control.v1`。
- 不复用已删除字段号。
- 删除字段时使用 `reserved`。
- 不原地制造 breaking change；需要时新增 `v2`。
- 未知字段必须保持兼容。
- 修改契约后生成 Rust、Swift、C# 和 TypeScript 输出。
- `generated/` 禁止手工编辑。
- CI 应在生成后检查工作区没有差异。

验证：

```bash
buf format --diff --exit-code
buf lint
buf breaking --against '.git#branch=main'
```

若仓库还没有基线 commit，记录 breaking check 暂不可运行，不得伪造成功。

## 11. 数据库与迁移

- 只有 `gatewayd` 写 SQLite。
- UI、Provider、Adapter 不直接访问表。
- 已发布 migration 不得修改。
- schema 变化必须新增 migration。
- migration 必须有升级测试和失败恢复测试。
- 迁移失败不得删除或重建用户数据库。
- 策略写入、快照编译和 active 切换必须使用明确事务边界。
- 凭据只保存 Keychain/系统凭据库引用。
- 测试 fixture 必须脱敏，禁止复制真实订阅和用户数据库。

## 12. 浏览器扩展

- 默认使用 `activeTab` 和最小权限。
- 不请求浏览历史。
- 不采集页面正文。
- 发送 URL 前移除 query 和 fragment。
- Safari、Chromium、Firefox 共享领域逻辑，目标目录只处理 API 和打包差异。
- 学习得到的跨域候选不是自动可信规则；按置信度展示并让用户确认。
- 多标签页场景必须测试，不能把同一浏览器其他标签页流量归到当前网站。

## 13. 第三方适配器

- 只使用公开 API、公开配置格式和可验证的重载机制。
- 修改前只读检测版本和 capability。
- 修改前建立带 hash 的备份。
- 候选配置先验证，再原子应用。
- 应用后做真实路径验证。
- 失败自动回滚。
- 不支持的版本明确拒绝，不能 best-effort 盲改。
- 日志和错误不能输出订阅 URL、Token、密码或私钥。

## 14. 隐私和日志

- 默认只记录应用身份、域名/IP、端口、协议、策略和决策。
- 不记录 payload。
- 对域名、路径和用户标识支持脱敏。
- 日志有保留期限。
- 诊断包生成前自动脱敏并允许用户预览。
- 不自动上传诊断包。
- 新增遥测前必须更新隐私文档并获得产品明确授权。

## 15. 测试要求

按风险选择测试层级：

- 纯规则：单元测试和 property test。
- 契约：round-trip、未知字段和 breaking check。
- 数据库：真实 migration/integration test。
- Provider/Service：本地 fixture integration test。
- macOS/Windows 捕获：真实系统测试。
- 浏览器：多标签页、重定向、隐私模式和权限测试。
- 网络：IPv4、IPv6、TCP、UDP、DNS、QUIC。
- 故障：上游失败、DNS 失败、组件崩溃、睡眠唤醒、网络切换和回滚。

测试默认使用本地 fixture，不依赖付费节点、生产 VPN 或公共第三方服务。

修复 bug 时：

1. 先写能复现问题的测试。
2. 修复最小根因。
3. 扫描相同模块中的同类问题。
4. 运行定向测试。
5. 运行相关构建和门禁。

## 16. 证据与完成声明

区分：

- 配置证据：配置已生成。
- 决策证据：连接命中规则。
- 路径证据：连接使用物理接口或代理网关。
- 出口证据：远端探针观察到预期公网出口。

“已确认直连”至少需要路径证据。只有配置或单元测试时，必须明确标注尚未完成真实系统验收。

## 17. 文档和 ADR

以下变化必须新增或更新 ADR：

- 进程/组件边界。
- IPC 协议。
- 数据面帧协议。
- 策略优先级。
- 默认失败语义。
- DNS/Fake-IP 策略。
- 数据库权威来源。
- 协议核心或许可证路线。
- Windows Driver 引入。
- 支持平台最低版本。

架构实现变化必须同步更新 `docs/TECHNICAL_IMPLEMENTATION.md`。

## 18. Git 与变更范围

- 不使用 `git reset --hard`、`git checkout --` 或其他破坏性清理命令。
- 不提交、推送、创建 PR，除非用户明确要求。
- 用户要求“提交 git”时，只提交审查过的相关文件。
- 显式列出 staged 文件，不使用宽泛 `git add .`。
- 不提交开发证书、Provisioning Profile、Keychain 导出、真实节点或本机私有配置。
- 不修改与任务无关的锁文件。
- 发现用户改动与目标冲突时停止并说明，不覆盖。

## 19. 推荐工作流

1. 阅读文档和相关代码。
2. 确认改动属于哪个模块。
3. 检查现有测试和相似实现。
4. 对跨组件改动先更新契约/ADR。
5. 以小模块实现，不堆入入口文件。
6. 添加定向测试。
7. 运行格式和静态检查。
8. 运行相关单元/集成/系统测试。
9. 检查 `git diff --check` 和最终 diff。
10. 报告实际验证、未验证项和剩余风险。

## 20. Definition of Done

只有同时满足以下条件，任务才可声明完成：

- 符合产品不变量和依赖方向。
- 代码职责清晰，没有新增单文件耦合点。
- 有对应测试。
- 契约和 migration 兼容。
- 失败、超时、取消和回滚路径存在。
- 无敏感数据泄漏。
- 相关文档已更新。
- 定向验证通过。
- 相关全量门禁通过，或明确记录与本次无关的既有失败。
- 平台功能在真实目标上验收，或明确标注尚未平台验收。
