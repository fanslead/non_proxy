# Mihomo 适配器

该模块生成 `behavior: classical` 可引用的 YAML payload。主配置需要登记本地
rule-provider，并在原有兜底规则之前加入 `RULE-SET,<managed-provider>,DIRECT`。

应用规则使用完整 Bundle 路径的 `PROCESS-PATH-WILDCARD`，避免仅按进程名误匹配。
当前模块只生成经过转义的候选内容；版本检测、原生配置校验、备份、原子应用、重载、
验证和回滚由 adapter-host 负责。
