# ADR-0007：使用 WFP 截获 Windows 明文 DNS，不修改网卡 DNS

- 状态：Accepted
- 日期：2026-07-31
- 决策范围：Windows DNS 接管与任意 VPN 共存

## 背景

把每块网卡的 DNS 主服务器改成 loopback 看似简单，但会覆盖 DHCP、VPN
profile、DoH 属性和企业策略的来源语义。服务崩溃、VPN 重连、网卡切换与升级
回滚时，很难证明“恢复后的值与原始配置完全一致”。同时，部分 VPN 使用独立
虚拟接口或 profile DNS，只改物理网卡无法保证系统 resolver 会经过本地服务。

Windows Filtering Platform 的 ALE connect redirect 支持 TCP 和 UDP。已有
NonProxy Driver、动态 BFE session 和 Service PID 递归排除机制可以把远端 53
端口的明文 DNS 定向到本地随机端口，而不持久修改网卡。

## 决策

- 动态 BFE session 为 IPv4/IPv6 各安装三类 filter：
  - TCP + remote port 53，DNS context，高优先级；
  - UDP + remote port 53，DNS context，高优先级；
  - 普通 TCP，TCP context，较低优先级。
- Driver 配置 ABI 使用独立的 `DNS_REDIRECT` 与 `TCP_REDIRECT` flag，以及
  IPv4/IPv6 TCP/DNS 四个 loopback 端口。
- Service 先绑定随机 DNS 端口并只启用 DNS redirect；系统 resolver 的随机
  `.invalid` A 查询必须收到唯一探针地址，DNS Provider 才能确认策略。
- 普通 TCP 只有在 DNS 探针和活动策略同时就绪后才启用。探针失效时退回
  DNS-only，而不是关闭 DNS 后形成无法自恢复的循环。
- 没有活动策略时，本地 DNS 对普通查询走物理 DIRECT；不得因 DNS-only 启动
  阶段返回全局 SERVFAIL。
- Service 自身 DNS 出站按 PID 排除，继续绑定物理接口，避免递归重定向。
- 停止或控制 handle 异常关闭会全量 disabled；动态 BFE handle 关闭后 filter
  自动消失，不存在网卡 DNS 恢复事务。

## 安全与兼容边界

- filter 只匹配 TCP/UDP 53，不捕获 DoH、DoT、mDNS 或任意 UDP。
- 应用内置 DoH/DoT、企业 NRPT、加密 DNS profile 或更高优先级 WFP 产品可能
  绕过或阻止探针；此时网站规则必须显示 Degraded，不能降级应用安全配置。
- DNS UDP redirect 的反向元组、TCP fallback、VPN filter 顺序和服务异常退出
  必须在真实 Windows 上形成数据包与 Driver 证据。交叉编译不能替代该验收。
- 通用 UDP/QUIC 是 [ADR-0008](0008-divert-windows-udp-datagrams.md) 的独立
  数据面，不因本 ADR 或 DNS 探针通过而自动获得真实环境验收。

## 参考

- [Using Bind or Connect Redirection](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/using-bind-or-connect-redirection)
- [FWPS_CONNECT_REQUEST0](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/fwpsk/ns-fwpsk-_fwps_connect_request0)
- [UDP packet flows](https://learn.microsoft.com/en-us/windows/win32/fwp/udp-packet-flows)
