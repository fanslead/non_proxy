# nonproxy-adapter-integration

该 crate 负责把 NonProxy 专属规则 sidecar 安全挂到用户明确选择的第三方客户端主配置中。
它是纯内存、无文件 I/O 的候选生成内核：调用方提供原配置和显式路径，得到可校验的候选、
SHA-256、规则引用名和实际 direct 目标；文件备份、原子应用、重载与回滚由
`nonproxy-adapter-transaction` 和 `adapter-host` 协调。

当前接入格式：

- Surge：在 `[Rule]` 的最前方维护带唯一标记的
  `RULE-SET,<relative-path>,DIRECT`，保留原换行风格和其他行。
- Mihomo：在现有 `rule-providers` 下追加唯一的本地 classical provider，并把对应
  `RULE-SET,<provider>,DIRECT` 放到 `rules` 首位。YAML 只做定点文本插入，随后用完整
  YAML parser 重新验证；注释、凭据和无关节点不会被重排或重新序列化。
- sing-box：通过 JSONC CST 在 `route.rule_set` 和 `route.rules` 首位追加独占对象，保留
  注释、尾逗号、缩进和无关字段。只有唯一 direct outbound 时才自动选择；多个 direct
  outbound 必须由用户明确选择 tag。

安全边界：

- 主配置最多 2 MiB，接入标识和 direct tag 有界且拒绝控制字符。
- sidecar 必须位于主配置目录内，并使用仅包含 ASCII 字母数字、`/._-` 的相对引用，
  避免 Mihomo `SAFE_PATHS` 扩权和配置语法注入。
- 只修改 NonProxy 独占名称或标记；同名但内容不同、重复标记、错误容器类型和歧义 direct
  出口一律失败，不覆盖用户配置。
- `patch` 后必须再次 `inspect`，幂等重放不产生新差异。
- 该 crate 不读取文件、不运行客户端、不处理 API secret，也不把配置引用冒充重载或路径证据。
