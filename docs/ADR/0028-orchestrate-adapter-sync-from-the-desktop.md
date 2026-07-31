# ADR-0028：由桌面端以失败关闭方式编排第三方客户端同步

- 状态：Accepted
- 日期：2026-08-01

## 背景

adapter-host 已能安全生成、验证、写入、重载和恢复第三方客户端配置，但它不拥有策略目录、
平台应用目录或用户交互状态。桌面端若直接把策略列表逐条拼接成规则，会混用草稿与活动修订；
若把应用加目标、网络、端口或传输条件拆开，会把窄规则扩大成全应用或全目标直连。

## 决策

1. 桌面端使用独立的 Adapter UDS 和 `adapter.capability`，不复用 gatewayd 的控制通道或
   `session.capability`。macOS 端点固定派生自共享状态根目录的 `adapter-host` 私有子目录；
   Windows 继续通过平台注册点保留命名管道实现边界，未接入前显式不可用。
2. 同步只读取 `GetActivePolicySnapshot`。投影载荷使用 `normalized-policy-v1`，revision 等于
   活动快照版本，正文另算 SHA-256 后交给 adapter-host 复验。
3. 只投影能无损表达的单维 DIRECT 规则。应用规则必须由当前平台、稳定签名身份和唯一的
   已验证 Bundle 路径补全；域名与 CIDR 必须得到客户端版本能力支持。组合匹配、网络配置档、
   端口、传输、辅助进程、缺失路径或缺失能力任一出现时，整次同步在 prepare 前失败关闭，
   并返回逐规则 blocker，不生成更宽的替代规则。
4. 桌面按 detect → capabilities → active snapshot → project → prepare 的顺序编排。prepare
   完成后、apply 写入前再次读取活动快照，并以版本和 32 字节内容哈希共同检查漂移。漂移时
   候选不写入目标文件，等待宿主按期限清理恢复材料。
5. apply 必须同时确认文件应用与公开入口重载。随后 verify 若不能证明配置文件一致，则桌面
   立即以本次 change/backup 回滚并要求旧配置重新载入；恢复不完整时返回最高优先级错误。
6. `configuration_verified`、`path_verified` 和 evidence level 原样保持独立。首版同步成功只
   显示“配置已确认”，不得显示“已经直连”或“已绕过 VPN”。

## 结果

- 草稿漂移和投影降级都不会静默改变第三方客户端流量范围。
- UI、策略投影、RPC 传输和平台路径发现保持分层，后续 Windows 只需替换平台传输与应用目录。
- prepare 可能在快照漂移时留下未应用的限时恢复材料；它不触碰目标配置，由 adapter-host 的
  既有过期清理回收。
- 真实请求路径和公网出口证据仍是下一阶段能力，不能由本编排的配置成功推断。
