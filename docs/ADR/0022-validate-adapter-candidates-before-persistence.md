# ADR-0022：第三方候选必须在持久化前通过客户端原生校验

## 状态

已接受。

## 背景

确定性 renderer 能阻止规则注入并生成已知格式，但不能证明某个已安装客户端版本会接受
候选。若先写入 sidecar 再调用客户端校验，进程崩溃可能留下未经验证却可被后续误用的
change；若直接用用户主配置验证，又会把订阅凭据和无关配置带入宿主临时文件。

## 决策

1. `prepare` 在任何 candidate、backup 或 manifest 持久化之前先执行同版本预渲染。原生
   校验成功后，事务内核用相同输入再次渲染；最终 hash 与规则数必须和预渲染完全一致。
2. 校验工作区由宿主创建为短期私有目录，候选和合成配置为 `0600`。子进程不经 shell，
   清空继承环境，只设置 `LANG=C` 和指向隔离目录的 `HOME`，stdin 关闭，最长五秒，
   stdout/stderr 各有 64 KiB 上限。退出后临时目录由宿主删除。
3. Surge 从用户选择的 App Bundle 推导并再次校验随包
   `Contents/Applications/surge-cli`，使用 `-c` 检查只引用候选 ruleset 的最小 profile。
4. Mihomo 使用 `-t -d <isolated-directory> -f <synthetic-config>`。合成配置只包含本地
   classical rule-provider、`RULE-SET,...,DIRECT` 和 `MATCH,DIRECT`，不复制用户节点、
   订阅、API secret 或其他主配置。
5. sing-box 使用 `rule-set compile --output <temporary.srs> <candidate.json>`。除了成功
   退出，还必须生成非空、非符号链接且不超过 4 MiB 的普通输出文件。
6. 客户端返回失败状态映射为非重试的候选无效；启动、IO、后台任务、超时或超量输出
   映射为可重试的校验不可用。RPC 只返回稳定错误码，不返回客户端 stdout/stderr 或路径。
7. 原生校验不等于主配置已引用、客户端已重载或真实流量命中。成功响应只设置
   `client_validated=true`，不能提升路径证据等级。

## 依据

- Surge Mac CLI 公开 `-c <profile-path>`，官方文档：
  https://manual.nssurge.com/others/cli.html
- Mihomo 的 rule-provider `type: file`、`behavior: classical` 与 HomeDir 路径限制：
  https://wiki.metacubex.one/en/config/rule-providers/
- sing-box 官方 source rule-set 编译命令：
  https://sing-box.sagernet.org/configuration/rule-set/source-format/

## 后果

- 渲染器测试和客户端 parser/CLI 形成两道独立门禁，客户端版本变化仍由 apply 前复检阻断。
- 校验不需要读取或复制用户主配置中的敏感字段。
- 后续主配置事务、重载和路径验证仍必须分别实现并提供回滚；本 ADR 不把候选校验冒充
  完整适配器可用性。
