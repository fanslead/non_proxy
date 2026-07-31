# nonproxy-adapter-host

`adapter-host` 是第三方客户端适配器的独立低权限进程。它只接受用户明确登记的客户端
路径，使用私有本地 RPC 和独立会话能力文件，负责版本检测、能力降级和可恢复 sidecar
事务。候选在写入事务状态前必须通过客户端原生工具校验：Surge 使用随包
`surge-cli -c`，Mihomo 使用 `-t`，sing-box 使用 `rule-set compile`。

当前服务不会修改第三方主配置、读取订阅凭据或声称热重载/真实路径已经验证。桌面端在
剩余客户端专属门禁完成前不得显示“已接管”。
