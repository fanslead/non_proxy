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
