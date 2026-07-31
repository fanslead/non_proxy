# 桌面端首批测试状态

- [x] 研究完成
- [x] 测试计划完成
- [x] 测试实现编译
- [x] 窄范围测试通过
- [x] 测试发现通过
- [x] 完整桌面 solution 通过
- [x] 断言质量复查完成
- [x] 覆盖缺口复查完成

## 复查结论

- 七个发现项均有行为断言，不包含空测试或仅验证“无异常”的弱断言。
- ViewModel、组合根和 AXAML 无头加载相互隔离，测试之间不共享可变状态。
- 当前覆盖只证明共享桌面骨架；macOS System Extension、Windows Service/WFP 和真实控制 RPC 尚未实现，因此不把这些能力计为已覆盖。

## Windows 本地传输与 Service 批次状态

- [x] 限定范围研究与静态配对分析
- [x] C# 端点安全边界测试
- [x] Rust Windows 配置和 DACL 门禁测试
- [x] Windows IPC/管道/Service target-only 测试
- [x] server ready 生命周期实跑断言
- [x] Windows x64 与 arm64 测试代码 target compile
- [x] 完整 gatewayd 与 Desktop Tests
- [x] 格式、diff 和断言质量复核

### 干净验证

- `cargo test -p nonproxy-gatewayd`：51 个库测试、9 个集成测试、1 个 Provider RPC 测试全部通过。
- `dotnet test apps/desktop/NonProxy.Desktop.Tests/NonProxy.Desktop.Tests.csproj -c Release --no-restore`：57/57 通过。
- `cargo check -p nonproxy-windows-ipc --tests --target x86_64-pc-windows-msvc`：通过。
- 设置仅用于 `cargo check` 的临时 SQLite link metadata 后，`nonproxy-gatewayd --tests` 在 `x86_64-pc-windows-msvc` 与 `aarch64-pc-windows-msvc` 均通过；该检查没有链接或运行 Windows 二进制。
- `cargo fmt --all -- --check`、定向 `dotnet format --verify-no-changes`、`git diff --check`：通过。

### 断言质量与剩余缺口

- 每个新增断言都能抵抗至少一个合理变异：长度 `>`/`>=`、命名空间大小写放宽、双传输误配、开发 DACL 被 Service 接受、pending checkpoint 被清零、Running 错误接受 `SHUTDOWN`、ready 过早或不发送。
- 环境变量测试使用 `finally` 恢复原值；当前测试仓库没有其他用例操作同名变量。
- Windows 控制工厂的真实 Named Pipe 连接、取消后句柄释放、Windows 组合根解析和 SCM 状态上报仍需真实 Windows 测试项目/系统验收。
- 本批次没有证明 WFP、Driver、真实流量路径或出口。

## 出口健康测试批次状态

- [x] Research 与 Plan
- [x] Protocol、Gateway、Desktop RPC/service/UI 实现
- [x] 17 个新增行为测试
- [x] test-gap-analysis 与 assertion-quality 等价复核
- [x] Rust、.NET、Swift 完整相关回归
- [x] 契约生成、兼容、格式、lint 与完整桌面打包门禁

### 干净验证

- `cargo test --workspace`：全仓通过；其中 gatewayd 62 个库测试、9 个集成测试、1 个 Provider RPC 测试通过。
- `dotnet test apps/desktop/NonProxy.Desktop.Tests/NonProxy.Desktop.Tests.csproj -c Release --no-restore`：70/70 通过。
- `swift test --package-path platform/macos --disable-sandbox`：XCTest 87/87、Swift Testing 28/28 通过。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`dotnet format ... --verify-no-changes`：通过。
- `just contracts`、`just contracts-swift`、`just contracts-breaking`：C#/Swift 生成物一致，Buf 对 `HEAD` 无破坏性变更。
- gatewayd 测试代码在 `x86_64-pc-windows-msvc` 与 `aarch64-pc-windows-msvc` target check 通过；这是编译检查，不代表 Windows 链接或运行。
- `dotnet build apps/desktop/NonProxy.Desktop.slnx -c Release --no-restore`：Windows 宿主、macOS x64/arm64 宿主、universal System Extension、Safari App Extension、Native Messaging Host 与签名后 Bundle 校验全部通过，0 warning、0 error。

### 缺口复核与修正

- 补齐健康缓存恰好 60 秒边界、容量淘汰、出口 revision 变化、1/30 秒边界与缺省 5 秒。
- 补齐超时/连接失败后的 `Failed` 状态、稳定脱敏错误，以及 .NET 端缺失或超过 30 秒延迟的协议拒绝。
- Release 门禁暴露并修复两个既有打包阻塞：`main.swift` 与 `@main` 的 Xcode 入口冲突；Release 优化下 `nm -g` 无法证明 Swift Principal Class，改由 Objective-C runtime 元数据验证。

### 断言质量

- 17 个新增测试（Rust 8、.NET 9）均包含结果、状态、副作用、边界、异常或负向断言；没有空测试、只跑不验测试或永真断言。
- 测试覆盖正常握手、鉴权拒绝、连接失败、超时、过期、revision 不匹配、容量边界、协议畸形、单行 UI 更新以及 macOS/Windows 共享视图入口。
- 主要可抵抗变异包括：`>= 60 秒` 被误写为 `> 60 秒`、timeout 上限被放宽、旧 revision 被复用、失败仍保留 Ready、UI 误更新全部行、成功文案夸大为公网出口验证。
- 静态配对结果仅是源文件到测试文件的解析启发式，不代表行/分支覆盖率；真实公网出口 IP、最终策略路径以及 Windows 真机网络栈仍不在本批证明范围。

## 默认代理原子发布批次状态

- [x] 缺口研究、事务设计与测试计划
- [x] V7 migration 与权威 routing repository
- [x] 默认决策编译、原子发布与回滚恢复
- [x] Control RPC、C#/Swift 契约生成
- [x] 共享 Desktop service/ViewModel/UI
- [x] Rust 定向测试
- [x] .NET 定向测试
- [x] 契约、格式、lint 与全仓回归
- [x] macOS/Windows 打包门禁
- [x] 提交前 diff、缺口与断言质量复核

### 干净验证

- `cargo test --workspace`：全仓通过；其中 gatewayd 65 个库测试、11 个集成测试、1 个 Provider RPC 测试通过。
- `dotnet test apps/desktop/NonProxy.Desktop.slnx --no-restore --configuration Release`：78/78 通过。
- `pnpm run test`：28/28 通过；`pnpm run typecheck`、`pnpm run lint`、`pnpm run format:check` 通过。
- `swift test --package-path platform/macos --disable-sandbox`：XCTest 87/87、Swift Testing 28/28 通过。
- `just contracts`、`just contracts-swift`、`just contracts-breaking`：C#/Swift 生成物一致，Buf 对 `HEAD` 无破坏性变更。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`dotnet format ... --verify-no-changes`：通过。
- `nonproxy-windows-ipc --tests` 在 `x86_64-pc-windows-msvc` target check 通过；设置仅用于 `cargo check` 的临时 SQLite link metadata 后，`nonproxy-gatewayd --tests` 在 Windows x64 与 arm64 target check 通过。
- `dotnet build apps/desktop/NonProxy.Desktop.slnx -c Release --no-restore`：Windows 宿主、macOS x64/arm64 宿主、universal System Extension、Safari App Extension、Native Messaging Host、gatewayd 与签名后 Bundle 校验全部通过，0 warning、0 error。
- `just control-e2e`、`just native-messaging-e2e`、`just provider-e2e`：控制平面、Native Messaging、代理数据面及两个 Provider ACK/active 链路全部通过。

### 缺口与断言质量

- 原子事务测试同时断言 routing revision、默认出口与 pending snapshot，能识别“先改设置后发布失败”及其反向顺序造成的部分写入。
- 乐观锁、缺失/禁用出口、已有 pending、无效回滚源与不兼容能力均有负向断言；删除任一校验或把事务拆开都会使对应测试失败。
- RPC 测试断言鉴权、oneof、revision、稳定业务错误和状态/目录一致性；Desktop 测试断言跨页 revision、默认标记、pending 文案、恢复直连和 macOS/Windows 共享入口。
- 本批证明的是“期望默认路由配置与待发布快照”的一致性；真实已激活策略、签名系统扩展安装、其他 VPN 共存和真实公网出口仍需后续运行态诊断及真机验收。

## 决策证据入库、查询与桌面展示批次状态

- [x] V8 migration、证据模型与原子批量仓储
- [x] Provider 会话鉴权、权威快照重编译与决策复算
- [x] Control 分页查询、C#/Swift 契约生成
- [x] 桌面活动页证据等级、路径和保留总数展示
- [x] 幂等重放、冲突回滚、伪造决策、未来时间和证据越级负向测试
- [x] 格式、lint、契约、全仓回归与提交前 review

### 干净验证

- `cargo test --workspace`：全仓通过；其中 gatewayd 75 个库测试及全部集成/RPC 测试通过。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`：通过。
- `dotnet test apps/desktop/NonProxy.Desktop.slnx --no-restore --configuration Release`：82/82 通过。
- `swift test --package-path platform/macos`：XCTest 87/87、Swift Testing 28/28 通过。
- `pnpm test`：28/28 通过。
- `scripts/contracts/check.sh`、`scripts/contracts/check-swift.sh`：C#/Swift 生成物一致。
- Windows x64/arm64 全 workspace 交叉检查在 `libsqlite3-sys` 编译前置环境停止：当前 macOS 宿主缺少 MSVC/Windows C SDK，分别找不到 `stdlib.h` 与 `setjmp.h`；没有把该环境失败计为 Windows 编译通过。

### 断言质量与剩余缺口

- 仓储测试同时验证批次完全回滚和相同事件精确幂等，可识别“部分写入”与“同 flow 覆盖旧证据”变异。
- Gateway 不信任 Provider 上报结果，而是校验会话平台、快照状态/哈希并用权威编译快照重算；伪造动作、规则、理由或版本都会被拒绝。
- SQLite trigger、Rust 领域模型和 C# 映射三层均拒绝 `DECISION` 携带路径、失败记录冒充 `PATH`、直连缺接口、代理缺出口及无探针的 `EXIT`。
- 本批只打通可信入库、查询和显示链路。macOS/Windows 平台尚未在真实连接建立后生产 `PATH` 事件，因此当前不会虚构路径或公网出口证据；平台生产器属于下一独立批次。

## macOS/Windows 真实路径生产器批次状态

- [x] V9 fail-open 证据模型、数据库约束与桌面诊断
- [x] macOS Transparent Proxy 实际 PATH 生产与安全 fail-open 接管
- [x] macOS DNS 非缓存实际 PATH 与缓存 DECISION 语义
- [x] Windows TCP/UDP/DNS 实际 PATH 生产与 best-effort 上报
- [x] Provider 有界队列、稳定批次重试与丢失计数幂等
- [x] Provider 并发请求滑动防重放窗口
- [x] Rust、Swift、.NET、Windows target、契约和 lint 门禁
- [x] Universal Release Bundle 与三条跨语言 E2E
- [x] 提交前逐文件、规模、禁止用法和 staged diff 复核

### 干净验证

- `cargo test --workspace`：全仓通过；其中 gatewayd 85 个库测试、11 个
  gateway 集成测试和 1 个 Provider RPC 测试通过。
- `swift test --package-path platform/macos --disable-sandbox`：XCTest 98/98、
  Swift Testing 28/28 通过。
- `dotnet test apps/desktop/NonProxy.Desktop.slnx --configuration Release
  --no-restore`：85/85 通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 与
  `just format-check`：通过。
- Windows `x86_64-pc-windows-msvc`、`aarch64-pc-windows-msvc` 全 workspace
  target check 通过，arm64 gatewayd lib clippy 通过；检查使用宿主 SQLite
  metadata，仅证明条件编译与类型，不代表 Windows 链接或运行。
- `just contracts`、`just contracts-swift`、`just contracts-breaking`：C#/Swift
  生成物一致，Buf 对 `HEAD` 无破坏性变更。
- `dotnet build apps/desktop/NonProxy.Desktop.slnx -c Release --no-restore
  --no-incremental`：两个 Universal System Extension、Safari Extension、
  Native Messaging Host、gatewayd 和最终 App Bundle 均通过签名与结构校验，
  0 warning、0 error。
- `just control-e2e`、`just native-messaging-e2e`、`just provider-e2e`：控制平面、
  浏览器桥接、NPF1 代理数据面和两个 Provider 的 ACK/active 链路全部通过。

### 提交前审查修正

- 修正响应丢失后重试同一批次会重复累计 dropped event：稳定复用 `batch_id`，
  gatewayd 使用 4096 条有界历史去重。
- 修正 Windows 证据编码或队列失败可能经 `?` 反向中止转发：全部改为无返回值的
  best-effort 旁路并累计丢失诊断。
- 修正 DNS、心跳与决策 RPC 并发乱序会被误判重放：Provider 会话使用 4096 条
  有界滑动序号窗口，允许窗口内未见乱序，仍拒绝重复和过旧请求。
- 修正 fail-open DNS 缓存命中会隐藏原始代理失败或冒充新 PATH：改为携带稳定
  失败码的 `DECISION`。
- 修正 macOS 只有异步代理建链失败进入 fail-open、同步 endpoint 编码/relay 容量
  失败却直接拒绝：同步和异步的路径建立前失败统一交给同一恢复规划器。
- 复核代理预建立失败的回收顺序：先同步注销旧 relay，再把尚未打开的 NE flow
  交给 DIRECT；失败回调只执行一次。

### 证据边界与后续缺口

- 本批只生产 `DECISION` 和实际建立后的 `PATH`；`EXIT` 仍必须由独立公网探针
  观察后产生，当前没有伪造出口证据。
- macOS 通过双架构构建和 Bundle 签名校验，但尚未完成有正式 entitlement 的
  System Extension 真机安装、重启恢复及与第三方 VPN 共存流量验收。
- Windows 仅完成 x64/arm64 交叉类型检查，尚未在 Windows 真机验证 Service、
  WFP Driver、DNS 重定向和真实网络路径。
- Provider 被系统强制终止时，内存中的 best-effort 队列仍可能丢失；系统状态会
  显示已知 dropped event，但无法声称覆盖强杀前尚未计数的内存事件。

## macOS gatewayd 防回环信任边界批次状态

- [x] 固定 gatewayd 代码签名 identifier，并在正式签名时绑定 TeamIdentifier
- [x] 快照注入不可由低权限控制面创建或覆盖的 SYSTEM 直连规则
- [x] 旧 pending 原子替换、旧 active 保留和 Provider 绑定前升级
- [x] 旧 active 升级期间阻止 gatewayd 建立代理上游，ACK 后原子恢复
- [x] 历史回滚重建当前系统规则及默认路由
- [x] 普通策略/运行状态目录隐藏内部系统规则
- [x] macOS 打包、Bundle 校验和 gatewayd 启停冒烟
- [x] Rust/Swift/.NET、Windows 条件编译、契约、格式、lint 与跨语言 E2E

### 干净验证

- `cargo test --workspace`：全仓通过；其中 gatewayd 93 个库测试、11 个 gateway
  集成测试和 1 个 Provider RPC 测试通过。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all
  -- --check`：通过。
- `swift test --package-path platform/macos --disable-sandbox` 在仓库固定
  `scripts/bootstrap/env.sh` 工具链下通过：XCTest 98/98、Swift Testing 28/28。
- `dotnet test apps/desktop/NonProxy.Desktop.slnx --configuration Release
  --no-restore`：85/85 通过。
- Windows `x86_64-pc-windows-msvc` 与 `aarch64-pc-windows-msvc` 全 workspace
  `cargo check --all-targets` 通过；使用宿主 SQLite link metadata，只证明条件
  编译与类型，不代表 Windows 链接或运行。
- `just contracts`、`just contracts-swift`、`just contracts-breaking` 与
  `just format-check`：通过。
- `dotnet build apps/desktop/NonProxy.Desktop.slnx -c Release --no-restore
  --no-incremental`：Universal System Extension、Safari Extension、Native
  Messaging Host、固定 identifier 的 gatewayd 和最终 App Bundle 均通过签名及
  结构校验，0 warning、0 error。
- Release App 的 `gateway-bundle-smoke.sh` 通过；`just control-e2e`、
  `just native-messaging-e2e`、`just provider-e2e` 全部通过。

### 提交前审查修正

- 初版只在新快照追加系统规则，无法修复升级数据库中的旧 active/pending，历史
  回滚也会复制旧 payload；现已增加 Provider 绑定前的候选快照规范化和重建回滚。
- 初版只匹配固定 identifier，未约束可用于防伪的 TeamIdentifier；正式打包现从
  已签名二进制提取 signer，并校验宿主 App、LaunchAgent、快照匹配器身份一致。
- 定向集成测试发现内部 SYSTEM 规则误进入普通运行策略目录；现仅保留在不可变
  快照和 Provider 数据面。
- Release 门禁实际执行到签名身份比较时发现 Bash 条件换行错误；修正后直接复验
  Bundle、gatewayd 启停冒烟，并重新完成一次 `--no-incremental` Release 构建。
- Bundle 冒烟原先只传包指纹，正式签名包不会携带 TeamIdentifier 启动；现从同一
  LaunchAgent plist 读取可选 signer 并传给被测 gatewayd。
- 最终 review 发现已运行 Provider 可能在拉取升级快照前用缓存旧 active 抢先发起
  flow；现以 active 快照内容驱动进程内原子门，连接工厂在保护规则激活前统一返回
  `NP_FLOW_SYSTEM_SNAPSHOT_PENDING`，ACK 激活后才恢复代理连接。

### 断言质量与证据边界

- 新测试同时断言签名匹配、伪造身份旁路失败、平台隔离、系统规则规范化、旧
  pending 拒绝原因、新 pending 版本/内容、旧 active 保留、事务失败回滚和历史
  来源记录，以及升级门在 ACK 前关闭、ACK 后开启；没有只执行不验证或永真断言。
- 可抵抗的主要变异包括：删除 signer 约束、把旧 pending 拒绝和新快照写入拆成
  两个事务、升级时读取未发布数据库草稿、回滚复制旧 payload、把系统规则暴露到
  用户目录，以及 Bundle 忽略 identifier/TeamIdentifier 漂移。
- 当前 Release 使用临时签名，证明打包结构、固定 identifier 与无 TeamIdentifier
  的开发分支；正式 TeamIdentifier、System Extension 真机安装、外部 VPN 共存和
  真实物理网络防回环仍需正式签名设备验收。

## 独立签名出口探针批次状态

- [x] 签名回执、严格验签与 HTTPS 客户端
- [x] 最小 TLS 探针服务与隐私边界
- [x] 固定 endpoint/公钥安装配置
- [x] DIRECT/PROXY 网关探针编排
- [x] Provider EXIT 越级拒绝
- [x] C#/Swift/Rust 控制契约生成
- [ ] 出口回执持久化与查询
- [ ] Desktop 触发、结果展示与无配置状态
- [x] 真实 TLS 客户端集成测试
- [x] Windows 物理 TCP 双架构条件编译
- [ ] 探针服务部署清单
- [ ] 全仓测试、lint、打包与提交前 review

### 当前验证

- `cargo test -p nonproxy-exit-probe -p nonproxy-probe-server -p
  nonproxy-gatewayd --all-targets`：回执库 5/5、探针服务 2/2、gatewayd 99 个
  库测试、11 个集成测试和 1 个 Provider RPC 测试通过。
- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo fmt --all -- --check`：全仓测试、lint 和格式门禁通过。
- `dotnet test apps/desktop/NonProxy.Desktop.slnx -c Release --no-restore`：
  85/85 通过。
- `swift test --package-path platform/macos --disable-sandbox`：XCTest 98/98、
  Swift Testing 28/28 通过。
- `just contracts`、`just contracts-swift`、`just contracts-breaking`：C# 和
  Swift 生成物一致，Buf 对 `HEAD` 无破坏性变更。
- `cargo xwin check --workspace --all-targets --target x86_64-pc-windows-msvc`：
  全 workspace 通过，并发现、修正 Windows 端 host 所有权类型错误。
- `nonproxy-windows-network --all-targets` 在 x64/arm64 Windows target 均通过，
  覆盖新增物理 TCP 绑定实现。arm64 全 workspace 因 `cargo-xwin` 向 ring 的 clang
  命令传入 `/imsvc` 失败；这是宿主交叉工具链问题，没有计为全 workspace 通过。
- `dotnet build apps/desktop/NonProxy.Desktop.slnx -c Release --no-restore
  --no-incremental`：Universal System Extension、Safari Extension、Native
  Messaging Host、gatewayd 和最终 App Bundle 均通过签名与结构校验，0 warning、
  0 error。
- `just control-e2e`、`just provider-e2e`：控制平面、NPF1 代理数据面和两个
  Provider 的 ACK/active 链路通过。

### 基础批次提交前复核

- 真实 TLS fixture 同时穿过 rustls、HTTP/1、JSON、nonce 与 Ed25519 验签，不是
  只调用签名函数的替身测试。
- 复核并修正未配置探针却声明能力、永久配置错误被标为可重试、TLS 请求提前退出
  后后台连接任务未取消、签名/TLS 私钥解析期间临时字节未清零四个问题。
- 生产 Rust 新文件均低于 400 行，未新增 `unwrap`/`expect`；错误响应只返回稳定
  代码和中文操作提示，不泄露 endpoint、代理凭据或底层网络错误。
- 本基础提交仍不代表完整出口功能完成；持久化、桌面 UI、密钥轮换、部署清单与
  真实公网双路径验收继续在后续批次完成。
