# nonproxy-adapter-transaction

该 crate 是 adapter-host 的同步、可测试文件事务内核。旧的 `prepare`/`apply` 继续管理
单个 NonProxy 专属规则 sidecar；`preview_integrated`、`prepare_integrated` 和
`apply_integrated` 把 sidecar 与用户明确选择的第三方客户端主配置纳入同一恢复单元。

调用方必须先独占 adapter-host 运行时实例，再依次调用 `prepare`、`apply`、`verify` 和
`rollback`。双文件应用先写 sidecar 再写主配置，回滚反向执行；启动时只自动恢复能由候选
和备份哈希证明的半完成状态，检测到外部修改时绝不自动覆盖。主配置备份和候选仅保存在 owner-only
状态目录，原子替换保留原权限位。

`verify` 对集成变更要求两个目标都匹配候选哈希，`path_verified` 仍固定为 `false`。客户端
原生校验必须发生在 `prepare_integrated` 前；重载和真实路径验证属于服务层后续门禁。完整
理由见 [ADR-0024](../../docs/ADR/0024-coordinate-sidecar-and-main-configuration-transactions.md)。
