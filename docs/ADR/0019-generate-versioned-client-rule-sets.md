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

1. `nonproxy-adapter-api` 定义 `normalized-policy-v1`。输入最大 1 MiB、最多 4096 条规则，
   只接受 `DIRECT` 和单一应用、域名或 CIDR 选择器；未知字段、重复标识、路径/换行注入、
   无效域名与网段全部拒绝。
2. 应用选择器使用已经由平台签名目录确认的绝对 App Bundle 路径作为适配器提示，不把
   路径当作 NonProxy 自身的稳定应用身份。渲染前统一规范为带尾部 `/` 的 Bundle 前缀。
3. 三个适配器只负责确定性候选渲染：
   - Surge 输出不带策略名的外部 Ruleset；App Bundle 前缀仅对 Mac 6.0+ 开放。
   - Mihomo 输出 `behavior: classical` 的 YAML payload，应用使用
     `PROCESS-PATH-WILDCARD`。
   - sing-box 输出 source rule-set version 3，应用使用转义后的
     `process_path_regex`；route action 和 direct tag 留在用户主配置中。
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

## 后果

- 纯渲染逻辑可以使用脱敏 fixture 完成单元测试，不依赖本机安装第三方客户端。
- UI、控制面和适配器之间有稳定的最小契约，第三方版本差异不会进入策略编译器。
- 当前提交仍不是可用的客户端适配模式；真实配置事务和路径证据是后续明确门禁。
