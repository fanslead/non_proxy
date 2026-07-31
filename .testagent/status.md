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
