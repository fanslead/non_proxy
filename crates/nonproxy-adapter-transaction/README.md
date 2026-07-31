# nonproxy-adapter-transaction

该 crate 是 adapter-host 的同步、可测试文件事务内核。它只管理 NonProxy 专属规则
sidecar，不解析或直接修改第三方客户端主配置。

调用方必须先独占 adapter-host 运行时实例，再依次调用 `prepare`、`apply`、`verify` 和
`rollback`。`verify` 当前只确认配置哈希，`path_verified` 固定为 `false`；客户端原生
校验、重载和真实路径验证属于服务层后续门禁。
