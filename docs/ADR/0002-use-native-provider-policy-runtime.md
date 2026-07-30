# ADR-0002：macOS Provider 使用原生纯函数策略运行时

- 状态：已接受
- 日期：2026-07-30

## 背景

策略模型、语义校验、冲突检测、规范哈希和快照编译的权威实现位于 Rust。macOS Network Extension 需要在系统回调的毫秒级预算内完成纯内存决策。

直接把 Rust 静态库嵌入 Swift System Extension 会增加一层裸指针 FFI、跨语言所有权和崩溃边界；每条 flow 通过 IPC 查询 `gatewayd` 又会破坏回调时延、离线快照和主进程退出后继续工作的不变量。

## 决策

保留 Rust 作为策略编译和快照规范的唯一权威实现。macOS Provider 使用一个受限的 Swift 纯函数运行时消费已验证的 `CompiledPolicyPayload`：

- Provider 必须按 Rust 同一规范重新计算内容哈希。
- 运行时不得访问网络、数据库或磁盘。
- 规则层级、优先级、特异性和稳定标识决胜规则必须与 Rust 一致。
- 每次语义变更必须同时更新 Rust 单元测试和 Swift 黄金向量测试。
- Protobuf 快照是唯一跨语言边界，不复制可编辑领域对象或数据库模型。

## 结果

优点：

- 不在高权限进程引入裸指针 FFI 和双重内存所有权。
- Network Extension 可以在 `gatewayd` 和 UI 不可用时继续使用最后一份已验证快照。
- Swift 运行时可直接用 Address Sanitizer、Thread Sanitizer 和系统扩展测试宿主验证。

代价：

- 需要维护跨语言决策一致性测试。
- 策略匹配语义扩展前必须先设计稳定快照字段和黄金向量，不能只改一端。

如果实测显示 Swift 运行时无法达到性能或一致性目标，再新增独立 ADR 评估自动生成运行时或窄 FFI；不得临时在 Provider 中调用控制面逐 flow 判定。
