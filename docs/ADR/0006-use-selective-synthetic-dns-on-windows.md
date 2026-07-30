# ADR-0006：Windows 网站规则使用选择性合成 DNS

- 状态：Accepted
- 日期：2026-07-31
- 决策范围：Windows 域名身份恢复与网站规则

## 背景

WFP ALE Connect Redirect 能提供应用身份、进程和目标 IP，但建立 TCP
连接时通常已经没有原始 DNS 域名。CDN、Anycast 和共享主机使“一个 IP
对应一个网站”的推断不可靠；TLS SNI 又会被 ECH 隐藏，且不能覆盖非 TLS
协议。只保存 DNS 观察到的域名/IP 短期关系，也无法在多个应用同时访问同一
共享 IP 时恢复精确域名。

Windows 网站规则需要在连接抵达 WFP 时同时保留应用身份和域名身份，而不做
TLS MITM、不安装根证书、不读取业务数据。

## 决策

Windows 本地 DNS 只对“活动策略中确实需要域名身份”的 A/AAAA 查询返回合成
地址：

- IPv4 使用 `198.18.0.1` 至 `198.19.255.254`。启用前必须确认该地址池没有被
  本机路由、企业网络或 VPN 占用；冲突时本能力不可启用，不能覆盖现有路由。
- IPv6 为每次安装生成一个本地分配 ULA `/64`，首字节固定为 `fd`，并持久化
  保存；只在该前缀内分配与 IPv4 等容量的低位地址。
- 域名与地址的初始槽位由版本化 SHA-256 输入确定。发生碰撞时在一个
  `BEGIN IMMEDIATE` 事务内有界线性探测，不替换已有绑定。
- A 与 AAAA 独立分配；同一域名和地址族在绑定有效期内始终得到同一地址。
- DNS 回答 TTL 固定为 30 秒；持久化绑定在最后一次签发后至少保留 24 小时，
  避免服务重启或旧 DNS 缓存把合成地址解释成另一域名。
- 合成回答保留事务 ID、问题、RD、CD 和 EDNS，清除 AD，不声称 DNSSEC
  验证成功。
- 非域名规则、局域网发现、Captive Portal 和普通域名查询继续走实际上游，
  不进行全局合成。

WFP 用户态代理收到合成目标地址后，从 SQLite 反查规范化域名，再使用
“真实应用身份 + 域名 + 端口 + 协议”执行同一份活动策略快照：

- PROXY 使用域名交给远端代理解析。
- DIRECT 必须通过绑定物理接口的 DNS 上游解析真实地址，再使用同一物理接口
  建立连接；不得调用已经返回合成地址的系统 resolver。
- 找不到绑定、绑定过期、地址池冲突或解析失败时明确拒绝，不把合成地址发往
  公网。

`nonproxy-dns` 只负责地址空间和 DNS 报文，`nonproxy-storage` 只负责配置与
绑定事务，策略快照只回答某域名是否需要保留身份。Windows DNS listener、
网卡 DNS 配置事务和 WFP 连接恢复留在平台/Service 层。

## 安全与能力边界

- 地址分配不代表对应域名可信；规范化、策略匹配和实际解析仍使用各自的受信
  边界。
- 本地合成回答不是权威 DNSSEC 证明。强制 DNSSEC 验证的应用可能拒绝该回答，
  应报告为不兼容，不能静默降低应用安全设置。
- 应用自带 DoH/DoT、硬编码 IP、绕过系统 DNS 或强制 ECH 的场景可能拿不到
  域名身份；应用级规则仍可工作，网站级规则不能虚假承诺覆盖。
- HTTPS/SVCB 查询需要返回无地址的成功回答，促使客户端继续查询 A/AAAA；
  该兼容行为必须单独测试后才能启用。
- 绑定记录是敏感元数据，诊断导出默认只给出散列/计数，不导出完整域名清单。
- 本 ADR 不代表 Windows DNS listener、DNS 设置安装事务、UDP/QUIC、驱动签名
  或真实 VPN 共存验收已经完成。

## 参考

- [RFC 2544：198.18.0.0/15 基准测试网络](https://www.rfc-editor.org/rfc/rfc2544.html)
- [RFC 6890：特殊用途地址注册表](https://www.rfc-editor.org/rfc/rfc6890.html)
- [RFC 4193：IPv6 Unique Local Addresses](https://www.rfc-editor.org/rfc/rfc4193.html)
- [RFC 5625：DNS Proxy 实现要求](https://www.rfc-editor.org/rfc/rfc5625.html)
- [RFC 4033：DNSSEC 安全介绍与要求](https://www.rfc-editor.org/rfc/rfc4033.html)
