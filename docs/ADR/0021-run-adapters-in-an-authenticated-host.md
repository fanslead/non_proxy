# ADR-0021：第三方适配器运行在独立认证宿主

## 状态

已接受。

## 背景

第三方客户端版本检测会执行用户明确选择的本地程序，sidecar 应用会写客户端可读取的
文件。把这些操作放进桌面 UI 或 `gatewayd` 会扩大故障和攻击面，也无法阻止未认证的
本机进程调用高风险写操作。

适配器不能扫描任意端口、猜测配置目录或读取订阅凭据。登记、检测、能力判断和文件事务
需要同一条可测试契约，同时必须继续区分配置证据与真实路径证据。

## 决策

1. `services/adapter-host` 是独立用户级低权限进程。macOS/Unix 首发只监听状态目录内的
   `0600` UDS；活跃套接字阻止第二实例，退出时只删除本进程创建的 inode。macOS App
   内嵌固定签名 identifier 的独立二进制与 LaunchAgent plist，签名二进制和源模板共同
   形成包指纹；宿主就绪后写入绑定包指纹、PID 和版本的 `0600` 运行身份。
2. 宿主启动时生成独立的 32 字节 `adapter.capability`。每个 RPC 都携带经过统一校验的
   operation ID 和能力令牌；令牌使用固定形状比较。认证文件逻辑位于共享
   `nonproxy-local-auth`，`gatewayd` 同步复用该内核，避免两套权限语义漂移。
3. 安装项只来自用户明确选择的客户端可执行文件和 NonProxy 专属 sidecar 路径。目录最多
   32 项，保存为 `0600`、有界、原子替换的版本化 JSON；重复登记相同内容幂等，同一 ID
   指向不同路径或客户端时拒绝覆盖。移除登记不删除第三方配置或事务恢复材料。
4. 可执行文件先规范化并验证为可执行普通文件。Surge 从其 App Bundle 的
   `CFBundleShortVersionString` 读取版本；Mihomo 调用 `-v`；sing-box 调用 `version`。
   子进程不经 shell、清空继承环境、stdin 关闭、最长三秒、stdout/stderr 各不超过
   64 KiB。输出只按对应客户端的固定前缀解析，不从任意数字猜版本。
5. 每次能力读取、`prepare` 和 `apply` 都重新检测版本。准备清单绑定客户端类型和精确
   版本；应用前安装项必须仍存在且当前版本完全一致。版本升级后能力由当前版本重新计算，
   低于 renderer 门限或事务窗口内发生升级时 fail-closed，不继续套用旧候选。
6. `prepare/apply/verify/rollback` 通过独立 RPC 调用
   `nonproxy-adapter-transaction`。策略正文带 SHA-256，hash 不匹配时不生成候选；同步文件
   IO 在 blocking task 边界运行。prepare 在持久化前先确定性预渲染并执行客户端原生校验，
   原生校验失败时不会产生可应用 change；具体边界见 ADR-0022。
7. 当前 `apply` 只原子应用专属 sidecar，`reloaded` 固定为 false；`verify` 最多返回
   `EVIDENCE_LEVEL_CONFIGURATION`，顶层 `verified` 与 `path_verified` 固定为 false。
   桌面端不得把这一步显示为“已接管”或“已经直连”。

## 当前边界

- 尚未修改或引导第三方客户端主配置引用 sidecar。
- 尚未接入第三方主配置、公开重载 API、实际决策/出口路径验证和失败后的服务层自动回滚
  编排。
- Windows 将复用 Protobuf、目录和事务领域语义，但命名管道、ACL 与进程创建限制需要
  独立实现和系统验收；当前非 Unix 服务会明确拒绝启动。
- NonProxy 不捆绑、下载或托管 Mihomo/sing-box 核心；这里只调用用户明确选择的既有
  程序执行只读版本命令。

## 后果

- 第三方客户端崩溃、挂起和超量输出不会拖住 UI 或 `gatewayd`。
- 本机调用方必须同时拥有私有 UDS 访问权和当前会话能力，重复 RPC 保持幂等。
- 后续主配置引导、重载和路径验证可以继续留在宿主进程，不把第三方差异反向耦合
  到策略引擎或桌面 ViewModel。
