# Surge 适配器

该模块把经过统一契约校验的 `DIRECT` 规则渲染成 Surge 外部 Ruleset。主配置应以
`RULE-SET,<managed-path>,DIRECT` 引用生成文件；生成文件本身不附带策略名。

Surge Mac 6.0.0 起才把以 `/` 开头并以 `/` 结尾的 `PROCESS-NAME` 作为 App Bundle
前缀匹配，因此更旧版本遇到应用规则会明确拒绝。当前模块只生成候选内容，不读取或
修改真实配置，也不把配置生成声明为路径验证。`adapter-host` 会把候选放入隔离临时目录，
使用 Surge App 随包的 `surge-cli -c` 校验引用该候选的最小 profile。主配置 `[Rule]`
独占块的无损候选接入由 `nonproxy-adapter-integration` 负责。
