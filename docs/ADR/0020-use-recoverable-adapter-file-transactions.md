# ADR-0020：第三方适配器使用可恢复文件事务

## 状态

已接受。

## 背景

候选规则渲染本身不会改变流量。adapter-host 后续需要把候选写到第三方客户端引用的
NonProxy 专属 sidecar；如果直接覆盖文件，应用崩溃、重复 RPC、客户端同时刷新配置或
用户手工编辑都可能丢失可恢复状态。

规则文件可能包含应用路径和域名，备份也属于本地隐私数据。事务错误不能把路径或文件
内容带进日志和 RPC。

## 决策

1. `nonproxy-adapter-transaction` 作为独立同步内核，由 adapter-host 在阻塞任务边界调用。
   UI、`gatewayd` 和渲染器均不直接写第三方客户端文件。
2. `prepare` 使用有界 `normalized-policy-v1` 生成候选，读取当前 sidecar 作为备份，并把
   candidate、backup 和 change manifest 分别写入 owner-only 目录。文件写入和目录项都
   `fsync`；Unix 文件为 `0600`、目录为 `0700`。
3. operation ID 与 adapter ID 生成稳定 change ID。完全相同的重放返回第一次的过期时间
   和哈希；同一 operation ID 携带不同候选、路径或备份时 fail-closed。
4. change manifest 记录候选/备份 SHA-256、规范化托管路径、客户端类型和十分钟准备期限。
   manifest 不记录规则正文。启动恢复扫描验证所有引用哈希，并清理没有 manifest 的崩溃
   孤儿文件；引用文件损坏时拒绝启动事务内核。
5. `apply` 只在当前 sidecar 仍等于 prepare 时的备份时执行同目录原子 rename。文件读取在
   Unix 使用 `O_NOFOLLOW`，大小前后均受 2 MiB 上限约束；符号链接、非普通文件和变化的
   外部内容均拒绝覆盖。应用后重新读取并核对候选 SHA-256。
6. `rollback` 只在当前内容仍等于本次候选时恢复原备份；若 prepare 前不存在 sidecar，
   则只删除本次候选。用户或客户端在 apply 后改过文件时保留现场和备份，交由用户选择，
   不强行覆盖。
7. 过期清理只删除“从未应用或已经恢复”的变更。已应用候选和发生外部修改的变更无论
   是否过期都保留恢复材料。显式清理同样要求当前内容已经恢复为备份。
8. `verify` 当前只产生配置哈希证据，`path_verified` 固定为 false。客户端原生 parser、
   公开热重载和真实连接路径验证未接入前，不能把事务成功显示成“已经直连”。
9. 内核错误只返回稳定 `NP_ADAPTER_*` 代码和通用中文消息，不包含实际路径或规则内容。
   adapter-host 还必须通过私有 UDS/命名管道保证每个状态目录只有一个写者。

## 当前平台边界

当前权限和 `O_NOFOLLOW` 约束完成 macOS/Unix 首发路径。Windows 复用领域事务和哈希
语义，但在正式启用前必须增加 ACL、ReplaceFile/MoveFileEx 语义与命名管道单写者验收。
该 crate 目前不修改第三方客户端主配置、不执行客户端二进制、不重载客户端，也不提供
路径证据。

## 后果

- 重复、超时和进程重启不会让 adapter-host 盲目覆盖未知内容。
- 自动回滚与用户外部编辑发生冲突时，优先保留用户现场和恢复材料。
- service/RPC 层可以独立实现版本检测和客户端原生验证，而不复制文件安全逻辑。
