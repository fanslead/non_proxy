# ADR-0031：用不可变快照承载有时限运行态路由覆盖

- 状态：Accepted
- 日期：2026-07-31

## 背景

产品需要“暂停 NonProxy 5 分钟”“全部直连 5 分钟”和“全部代理 5 分钟”。这些是应急运行
模式，不是用户的长期默认路由。如果复用 `SetDefaultRoute`，操作会永久修改
`routing_settings`，到期恢复还依赖桌面端或后台定时任务持续存活；进程崩溃、睡眠、时钟跳变
或并发发布都可能让临时状态无限延长或覆盖用户的新配置。

三种模式也不能合并为一个布尔开关：

- PAUSED 表示停止 NonProxy 的透明转发并回到当前系统路由；系统 VPN 可能仍然生效。
- DIRECT 表示使用 NonProxy 的物理网卡隔离直连路径，目标正是绕过第三方 VPN。
- PROXY 表示强制使用选定代理，失败时不得静默泄漏到直连。

macOS 的两个 Provider 还有不同回调语义。Transparent Proxy 拒绝一个新 flow 会让该流量继续
到最终目标，而 DNS Proxy 拒绝 flow 会终止 DNS。因此 DNS 暂停必须显式走 SYSTEM resolver，
不能照搬透明流量的返回值。

## 决策

1. 在 `CompiledPolicyPayload` v3 中增加可选 `RuntimeRoutingOverride`，包含严格模式、PROXY
   专用出口和毫秒精度绝对到期时间。存在标记及全部字段进入 Rust/Swift canonical hash。
2. `SetRuntimeOverride` 和 `ClearRuntimeOverride` 都从当前 active 快照重建一个新的 pending
   快照；携带并事务校验 `expected_active_snapshot_version`。不修改 `routing_settings`，不删除
   策略草稿，也不原地改活动快照。
3. 服务端接受 1 秒到 1 小时；桌面产品入口固定为 5 分钟。Compiler 同时验证到期范围和代理
   出口能力。数据面用快照中的绝对时间独立判断 `now < expires_at`，所以 UI、gatewayd 退出或
   设备睡眠不会延长覆盖。
4. 判定顺序固定为安全系统规则、活动运行态覆盖、普通规则。系统防回环和控制流量不能被
   “全部直连/代理/暂停”覆盖。
5. PAUSED 返回专用 bypass disposition。macOS Transparent Proxy 返回 `false`；macOS DNS
   Proxy 发起 SYSTEM DNS 请求；Windows TCP/UDP 使用系统路由 socket。旁路不生成普通策略
   decision record。DIRECT 走物理接口绑定路径；PROXY 生成指定出口的 fail-closed decision。
6. active 和 pending 覆盖必须分别展示。设置与取消都只有在所需 Provider ACK 后才生效；
   `pending_clears_override` 明确表示“恢复请求待确认，此刻旧覆盖仍可能生效”。
7. v1/v2 快照继续可读但不能携带覆盖字段。普通配置发布、默认路由修改、学习确认、系统规则
   升级和历史回滚只携带当前仍未到期的覆盖，不能复活历史或已到期覆盖。

## 结果

- 临时模式不会污染长期默认路由，也不会因为控制面进程退出而永久生效。
- PAUSED 与 DIRECT 在产品文案、数据面路径和证据模型上保持可核对差异。
- Provider ACK、并发版本校验和唯一 pending 约束继续复用现有快照发布不变量。
- macOS 与 Windows 共享 Protobuf、Rust 策略语义和 Avalonia UI；这不等于 Windows WFP
  实机网络路径已经完成验收，仍需按 Windows 系统验收文档验证。
