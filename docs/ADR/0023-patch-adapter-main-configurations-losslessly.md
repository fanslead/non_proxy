# ADR-0023：以独占节点无损接入第三方客户端主配置

- 状态：Accepted
- 日期：2026-08-01

## 背景

独立 ruleset/rule-provider 文件只有被正在运行的主配置引用才会生效。要求普通用户手写
Surge INI、Mihomo YAML 或 sing-box JSONC，既违背产品的统一配置目标，也容易把直连规则
放到兜底规则之后。另一方面，整份反序列化再序列化会丢失注释、改变格式，并可能重写含
订阅凭据的无关节点；按字符串猜测任意结构同样不可接受。

三种客户端的公开接入语义不同：

- Surge 的规则自上而下匹配，外部 Ruleset 由 `[Rule]` 中的
  `RULE-SET,<file>,DIRECT` 引用；
- Mihomo 的 classical provider 需要 `rule-providers` 定义和 `RULE-SET` 路由规则；
- sing-box 的本地 source rule-set 需要 `route.rule_set` 定义和指向实际 direct outbound
  tag 的 route rule，本地文件从 1.10.0 起支持变更自动重载。

官方依据：

- https://manual.nssurge.com/rule/ruleset.html
- https://manual.nssurge.com/overview/configuration.html
- https://wiki.metacubex.one/en/config/rule-providers/
- https://wiki.metacubex.one/en/config/rules/
- https://sing-box.sagernet.org/configuration/rule-set/
- https://sing-box.sagernet.org/configuration/route/rule/

## 决策

1. 新增纯内存 `nonproxy-adapter-integration`，输入客户端、稳定接入 ID、用户明确选择的主
   配置路径、sidecar 路径和可选 direct target；输出候选字节、哈希和去敏接入元数据。
2. sidecar 必须位于主配置目录树内。主配置只写 `./...` 相对引用，引用字符限制为
   ASCII 字母数字和 `/._-`，不依赖 Mihomo 的额外 `SAFE_PATHS` 权限，也不允许语法注入。
3. Surge 只维护带稳定起止标记的独占块，并把它放在 `[Rule]` 首位。重复、残缺或位于
   错误 section 的标记视为冲突。
4. Mihomo 不重新序列化整份 YAML。实现只在顶层 block mapping/sequence 的明确位置插入
   provider 和首条 route rule，保留原始文本；候选随后由完整 YAML parser 再次解析并按
   语义检查独占节点。非空 flow mapping/sequence 和同名冲突失败关闭。
5. sing-box 使用 JSONC CST 定点插入，保留注释、缩进和尾逗号。只有唯一带 tag 的 direct
   outbound 时自动选择；存在多个时要求显式 tag，并验证该 tag 确实对应 `type: direct`。
6. 独占 provider/tag 使用 `nonproxy-<integration-id>`。已存在且完全一致视为幂等；存在但
   任一字段不一致或路由到非 direct 目标时拒绝覆盖。
7. 所有候选必须通过同一引擎再次 `inspect`。本 ADR 只解决主配置候选，不授权直接写文件，
   不声明客户端已重载或真实流量已经走 direct。

## 结果

- 普通用户无需理解三种配置语法，UI 只需让其选择客户端、主配置和必要时的 direct 出口。
- 用户注释、订阅节点、API secret 和其他配置不会进入日志，也不会因全量序列化产生噪声。
- 后续可把“主配置候选 + sidecar 候选”纳入同一恢复事务，再调用公开重载并做路径验证。
- Mihomo 的复杂 flow-style 顶层容器不会被自动转换；第一版明确拒绝并提示用户选择标准
  block-style 主配置或由客户端导出兼容副本。
