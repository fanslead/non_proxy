# ADR-0025：把完整主配置接入绑定到适配器 RPC

- 状态：Accepted
- 日期：2026-08-01

## 背景

双文件事务内核只有被认证宿主调用才会进入产品链路。宿主此前只登记客户端可执行文件和
sidecar，RPC 也只传递 sidecar 哈希；即使内核能够生成主配置候选，调用方仍无法证明原生
校验、prepare 与 apply 针对的是同一主配置、直连出口和两个候选。

## 决策

1. 安装目录 v2 登记客户端可执行文件、必选主配置、位于主配置目录树内的 sidecar，以及
   可选的请求直连出口。v1 目录继续可读，但旧记录因缺少无法安全猜测的主配置而显示失败，
   用户必须重新登记；任何新写入都会升级为 v2。
2. `AdapterInstallation`、`RegisterInstallationRequest`、`PrepareChangeResponse` 和
   `ApplyChangeRequest` 只追加字段。prepare 返回 sidecar 哈希、主配置哈希、受管相对引用和
   实际直连出口；apply 必须回传两个哈希。主配置正文、备份和其中的凭据绝不进入 RPC。
3. prepare 在内存中生成完整集成候选，并把主配置候选与 sidecar 候选写入短期 `0700`
   工作区中的 `0600` 文件。Surge 对完整候选执行随包 `surge-cli -c`；Mihomo 执行
   `-t -d <isolated> -f <candidate>`；sing-box 先编译 source rule-set，再执行
   `check -c <candidate>`。子进程继续无 shell、清空环境、关闭 stdin，并受五秒和 64 KiB
   输出上限约束；输出及配置正文不写日志。
4. 原生校验成功后，宿主把两个预览哈希交给 `prepare_integrated`。内核重新读取主配置并重新
   生成候选；任一哈希、规则数、受管引用或实际出口变化都会删除尚未应用的 change 并失败。
5. manifest v4 在 v3 双文件信息上增加“请求直连出口”，同时继续读取 v3。apply 前重新检测
   客户端版本，并要求目录中的客户端类型、两个路径和请求出口与 manifest 完全一致，防止
   删除并同 ID 重登记后把旧候选应用到旧安装项。
6. apply 调用双文件事务；verify 只有两个候选哈希同时命中才提供配置证据。公开重载与失败
   自动恢复已在 [ADR-0026](0026-reload-adapter-clients-with-public-controls.md) 接入，成功 apply
   可返回 `reloaded=true`；路径证据尚未实现，因此 `path_verified=false`、顶层
   `verified=false`。

## 结果

- 普通用户只选择客户端和主配置；sidecar 引用、直连出口解析和两个候选的完整性由宿主统一
  管理，不需要理解三种规则语法。
- 主配置候选可能包含敏感字段，但只短期存在于私有临时目录和私有事务恢复目录，不经 RPC、
  stdout/stderr 或应用日志传播。
- 隔离工作区只物化本次主配置和 sidecar，不复制配置目录的其他文件。依赖额外相对本地文件
  的配置会在原生校验阶段失败关闭；后续若支持依赖物化，必须新增有界复制与防链接逃逸设计。
- 配置成功仍不单独代表客户端已加载；apply 必须再通过公开重载门禁。即使重载成功也不代表
  真实流量直连，下一阶段必须实现独立路径证据。
