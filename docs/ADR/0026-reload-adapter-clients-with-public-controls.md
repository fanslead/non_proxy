# ADR-0026：使用公开控制入口重载适配器客户端

- 状态：Accepted
- 日期：2026-08-01

## 背景

sidecar 与主配置成功写入，只能证明磁盘候选正确；客户端可能仍使用旧配置。若把“命令已发送”
当作重载成功，或者在重载失败后只恢复文件而不重新载入备份，运行态与磁盘态仍会分裂。三类
客户端提供的公开控制面不同，而且主配置可能包含 API secret，不能靠扫描端口、猜测进程或
输出配置正文来弥合差异。

官方依据：

- Surge CLI 提供 `reload` 和 `dump profile original`：
  https://manual.nssurge.com/others/cli.html
- Mihomo RESTful API 提供带 secret 的本地控制、`PUT /configs` 和 `GET /rules`：
  https://wiki.metacubex.one/en/api/
- Mihomo 控制器可配置 TCP、TLS、Unix Socket 或命名管道：
  https://wiki.metacubex.one/en/config/general/
- sing-box `run` 对 SIGHUP 重载配置，并在替换实例前执行配置检查；官方 systemd 单元同样以
  SIGHUP 实现 `ExecReload`：https://github.com/SagerNet/sing-box

## 决策

1. apply 在任何目标写入前依次执行只读事务预检、控制计划构建和客户端控制预检。事务预检
   重新验证有效期、两个候选哈希、候选文件和两个当前目标，但不创建 sidecar 或改写主配置。
   控制预检也必须能证明将要控制的正是用户登记主配置。
2. Surge 只调用所选 App Bundle 内的 `surge-cli`。预检要求
   `dump profile original` 的完整字节哈希等于事务备份；写入后调用 `reload`，再要求活动原始
   profile 的完整哈希等于主配置候选，并由接入引擎确认独占块位于正确位置。CLI 输出受
   2 MiB 和五秒限制，任何规范化或不一致都失败关闭。
3. Mihomo 只接受主配置中唯一的、字面量 loopback TCP `external-controller`。首版拒绝远程
   地址、TLS、Unix Socket 和命名管道，避免在尚无对应证书、peer credential 与 ACL 验证时
   扩大信任边界。secret 只从登记主配置读取，在请求生命周期内以可清零内存保存，不进入
   目录、RPC 或日志。控制计划要求读取配置的哈希等于事务备份，预检只以 `GET /version`
   验证鉴权控制通道，不提前改变运行态；写入后在调用前后验证磁盘候选哈希，以显式绝对
   `path` 调用 `PUT /configs?force=true`，并以 `GET /rules` 确认首条规则的受管 provider 名
   和直连策略。
4. sing-box 不猜 PID。宿主只选择与当前有效用户相同、可执行文件规范路径完全相等、命令为
   `run` 且恰好用一个 `-c/--config` 指向登记主配置的唯一进程；配置目录模式、多配置或多进程
   均拒绝。发送 SIGHUP 前后都要求主配置哈希等于本次候选或备份，并连续确认同一 PID、
   启动时间、可执行文件和配置绑定仍存活。此确认只证明受绑定进程未退出且磁盘配置未变化，
   不等于真实规则决策或出口证据。
5. 双文件 apply 成功后，重载或确认失败会立即以 manifest 中的 `backup_id` 回滚主配置和
   sidecar，再通过同一公开入口载入备份。`ApplyChangeResponse.rolled_back` 与
   `rollback_reloaded` 分别暴露文件恢复和旧配置重载结果；任一步无法证明完整恢复时返回稳定
   的恢复错误，保留事务材料供后续显式处理。若 apply 是对早先已应用候选的幂等重放，本次
   调用没有写文件，重载失败不得撤销既有变更；响应保留 `applied=true`、明确
   `reloaded=false`，由调用方重试或显式 rollback。手动 rollback 同样在文件恢复后重载旧配置。
6. 客户端与版本支持公开重载时暴露 `HOT_RELOAD` 能力，但 apply 仍会针对本次安装项和运行态
   执行全部门禁。`reloaded=true` 只表示客户端级加载确认；`path_verified` 继续为 false，
   桌面端不得把它显示为“已经直连”。

## 结果

- 正常成功不再停留在“文件写好了”，重载失败也不会故意留下候选运行态与备份磁盘态的组合。
- Surge 的活动 profile、Mihomo 的已加载首条规则和 sing-box 的精确进程身份提供不同强度的
  客户端级证据，响应只统一它们的“重载完成”含义，不抹平各自限制。
- Mihomo 控制预检是只读且经过鉴权的版本请求；候选仅在双文件事务成功后才进入重载调用。
- 真实请求是否命中直连规则、DNS 是否泄漏、物理接口和公网出口仍必须由独立路径验证完成。
