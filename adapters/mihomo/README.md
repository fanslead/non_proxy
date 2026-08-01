# Mihomo 适配器

该模块生成 `behavior: classical` 可引用的 YAML payload。主配置需要登记本地
rule-provider，并在原有兜底规则之前加入 `RULE-SET,<managed-provider>,DIRECT`。

macOS 应用规则使用完整 Bundle 路径的 `PROCESS-PATH-WILDCARD`；Windows Win32 应用使用
完整 `.exe` 路径的精确 `PROCESS-PATH`，避免仅按进程名误匹配。Windows 包系列身份没有
等价 Mihomo 字段时会拒绝渲染。
当前模块只生成经过转义的候选内容；`adapter-host` 在隔离 HomeDir 内构造只引用该候选的
最小配置并执行 `mihomo -t`，通过后才允许建立备份和可应用 change。原子应用、重载、
路径验证和回滚仍由宿主分阶段负责。主配置的无损候选接入由
`nonproxy-adapter-integration` 统一实现，不在本渲染器内读取 YAML。
