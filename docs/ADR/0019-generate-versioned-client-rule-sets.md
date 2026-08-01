# ADR-0019：先生成受版本约束的第三方客户端规则集

## 状态

已接受。

## 背景

Surge、Mihomo 和 sing-box 都能表达应用、域名和 CIDR 规则，但格式、应用匹配语义、
主配置挂载方式和重载机制不同。把字符串拼接直接放进 UI 或 `gatewayd` 会让第三方
客户端故障跨越进程边界，也容易把“文件写入成功”误报成“流量已经直连”。

产品还不能通过嵌入 sing-box 来回避其 GPLv3 义务。规则适配和协议核心是两个不同的
许可证边界，必须分开决策。

## 决策

1. `nonproxy-adapter-api` 定义版本化 normalized policy。输入最大 1 MiB、最多 4096 条规则，
   只接受 `DIRECT` 和单一应用、域名或 CIDR 选择器；未知字段、重复标识、路径/换行注入、
   无效域名与网段全部拒绝。
2. `normalized-policy-v2` 的应用选择器包含独立的 `selector_version`、`platform`、
   `path_kind` 和 `value`。macOS 只接受规范绝对 `.app` Bundle；Windows Win32 只接受
   无通配符、无 ADS、无相对分段的盘符绝对 `.exe`；Windows 包系列身份有独立种类，不能
   冒充可执行文件。路径只是由平台签名目录补全的适配提示，不替代 NonProxy 稳定应用身份。
   v1 继续兼容不含应用选择器的域名/CIDR 载荷，旧 `bundle_path` 不再被新应用投影接受。
3. 三个适配器只负责确定性候选渲染：
   - Surge 输出不带策略名的外部 Ruleset；App Bundle 前缀仅对 Mac 6.0+ 开放。
   - Mihomo 输出 `behavior: classical` 的 YAML payload；macOS Bundle 使用
     `PROCESS-PATH-WILDCARD`，Windows `.exe` 使用精确 `PROCESS-PATH`。
   - sing-box 输出 source rule-set version 3；macOS Bundle 使用转义后的
     `process_path_regex`，Windows `.exe` 使用精确 `process_path`；route action 和 direct
     tag 留在用户主配置中。
   - 两个 Windows 客户端都不把包系列身份降级为进程名；不能无损表达时返回 blocker。
4. 每份结果都包含客户端、格式、规则数与 SHA-256。候选生成不读取用户配置、不触发
   重载，也不产生高于“配置”的证据等级。
5. 后续 `adapter-host` 必须在独立进程中完成只读版本检测、带 hash 备份、客户端原生
   parser/CLI 校验、原子应用、公开重载、路径验证与失败回滚。未完成这一事务前，桌面
   UI 不开放“已接管”状态。
6. NonProxy 不捆绑或启动 sing-box 协议核心。若将来选择嵌入、随包分发或进程托管任一
   协议核心，必须另立许可证 ADR 并完成法律审查。

## 依据

- Surge 的规则按从上到下首个命中执行，外部 Ruleset 不含 policy；Surge Mac 6.0+
  才支持以 `/` 结尾的 App Bundle `PROCESS-NAME` 前缀。
- Mihomo 公开规则格式支持 `DOMAIN-SUFFIX`、`PROCESS-PATH` 系列和 CIDR。
- sing-box 1.11 引入 source rule-set version 3，公开 route rule 支持
  `domain_suffix`、`process_path_regex` 和 `ip_cidr`。

官方格式：

- https://manual.nssurge.com/rule.html
- https://manual.nssurge.com/rule/process.html
- https://wiki.metacubex.one/en/config/rules/
- https://sing-box.sagernet.org/configuration/rule-set/source-format/
- https://sing-box.sagernet.org/configuration/route/rule/
- https://sing-box.sagernet.org/migration/#190

## 后果

- 纯渲染逻辑可以使用脱敏 fixture 完成单元测试，不依赖本机安装第三方客户端。
- UI、控制面和适配器之间有稳定的最小契约，第三方版本差异不会进入策略编译器。
- 真实配置事务、客户端重载和路径证据仍是独立门禁；规则渲染成功本身不等于流量已直连。
