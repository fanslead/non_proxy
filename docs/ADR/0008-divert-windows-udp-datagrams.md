# ADR-0008：Windows 通用 UDP/QUIC 使用 WFP 数据报搬运

- 状态：Accepted
- 日期：2026-07-31
- 决策范围：Windows 远端 53 之外的 UDP、QUIC 与无连接 `sendto`

## 背景

NonProxy 必须让应用规则和网站规则同时覆盖 connected UDP、无连接 `sendto`
和基于 UDP 的 QUIC。把所有 UDP 都套用 ALE Connect Redirect 看似可以复用
TCP 代理，但 Microsoft 已记录：connected UDP 的 `connect` 与首个 `send`
可以位于不同 WFP 层，重定向到本地代理后数据报会被丢弃；官方 workaround
要求原应用改用无连接 `sendto`，桌面产品无法要求第三方应用配合。

修改网卡、虚拟网卡或 DNS 配置也不能得到每个数据报的应用身份；再引入一个
第三方 packet-divert driver 会增加第二套签名、升级、冲突和崩溃边界。现有
NonProxy WFP Driver 已是必须交付的最小内核组件，因此通用 UDP 应在同一驱动
内完成最小数据搬运，策略和网络出口继续保留在 Service。

## 决策

### WFP 分层

1. 动态 BFE session 在 `ALE_FLOW_ESTABLISHED_V4/V6` 为 UDP 安装 inspection
   callout，读取 PID 和有界 ALE App ID，并把身份 context 关联到对应 flow。
2. `DATAGRAM_DATA_V4/V6` 的 terminating callout 只匹配出站 UDP，排除远端
   53。明文 DNS 继续使用 ADR-0007 的专用 ALE redirect 和本地 DNS listener。
3. Driver 把 UDP 头后的 payload、原始本地/远端元组、compartment、接口索引、
   PID 和 App ID 编码为版本化记录，放入有界非分页队列，并吸收原数据报。
4. Service 经独占设备 IOCTL 批量取出记录，使用 App + 域名/IP + 端口执行共享
   策略。合成地址先反查持久化域名绑定。
5. DIRECT 使用绑定物理接口的 UDP socket；PROXY 使用 SOCKS5 UDP
   association；BLOCK 不产生回复。HTTP CONNECT 没有 UDP 能力，按该规则的
   fail-open/fail-closed 配置处理建立失败。
6. 收到回复后，Service 提交原始元组 context；Driver 构造 UDP/IP 头并调用
   `FwpsInjectTransportReceiveAsync0` 注入入站数据报。入站注入不会再次命中
   仅匹配出站方向的 divert filter。

Driver 不解析域名、策略、SQLite、SOCKS5、QUIC 或业务 payload。它只执行
身份关联、定界复制、队列、IP/UDP 构造和 WFP 注入。

### 会话与域名

- 会话键为 PID、App ID、原始本地地址和原始远端地址；相同四元组保持数据报
  顺序，不同会话可并行。
- 合成 DNS 地址恢复为规范域名后，DIRECT 通过物理 DNS 解析真实地址，PROXY
  把域名交给 SOCKS5 出口解析。
- 未经系统 DNS 的 DoH/DoT、硬编码 IP 或已缓存真实 IP 仍可执行应用/CIDR/
  端口规则，但无法凭空恢复原始域名。
- 空 UDP payload 合法并在 ABI、DIRECT、SOCKS5 和反向注入中保持。
- PROXY association 建立失败时可以按策略 fail-open 到 DIRECT；会话已经发出
  数据后不做中途重放，避免重复副作用。

### 有界资源

| 边界 | 上限 |
| --- | ---: |
| 单个 UDP payload | 65,000 bytes |
| 单次 Driver batch | 256 KiB |
| Driver 队列 | 4,096 records，记录总长约 16 MiB |
| Driver → Service channel | 256 datagrams |
| Service → Driver injection channel | 256 datagrams |
| 活动 UDP 会话 | 2,048 |
| 单会话后续队列 | 64 datagrams |
| 所有会话首包与排队 payload 总预算 | 32 MiB |
| 会话空闲回收 | 120 秒 |

队列已满、身份缺失、畸形头或分配失败时，已启用的通用 UDP 数据报 fail-closed
并增加 drop 计数，避免用户明确要求 DIRECT/BLOCK 的流量静默泄漏回 VPN。
Service/控制 handle 退出会先清除驱动 enable flag，动态 BFE session 随 engine
handle 关闭而删除，后续新流量恢复操作系统原路径。

## 被否决方案

- **全部使用 ALE UDP redirect**：connected UDP 存在系统已确认的丢包边界，
  不能覆盖任意第三方应用。
- **修改网卡 DNS 或路由**：无法可靠关联应用，且与 VPN、DHCP、NRPT 和恢复
  事务冲突。
- **额外引入通用 packet-divert driver**：增加第二套内核供应链和 filter
  排序边界，没有减少现有 WFP Driver 的必要性。
- **在内核执行策略/代理**：扩大攻击面、不可维护，也破坏共享策略唯一来源。

## 验收边界

当前仓库证据只包含 C/Rust ABI 一致性测试、Rust 单测和 Windows 用户态交叉
编译。以下全部通过前，不能宣称 Windows UDP/QUIC 已在真实环境完成：

- Windows WDK x64/ARM64 Driver 构建与签名；
- Driver Verifier、休眠/唤醒、服务崩溃和卸载；
- connected UDP、`sendto`、空数据报、IPv4/IPv6、QUIC 的双向元组证据；
- 物理 DIRECT 与至少两类第三方 VPN 的出口 IP 差异证据；
- 高速 UDP/QUIC 背压、丢包、内存和长时间稳定性测试。

## 参考

- [Microsoft：本地代理重定向 connected UDP 可能失败](https://learn.microsoft.com/pt-br/troubleshoot/windows-hardware/drivers/redirection-connected-udp-traffic-local-proxy-fail)
- [WFP Packet Injection Functions](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/packet-injection-functions)
- [FwpsInjectTransportReceiveAsync0](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/fwpsk/nf-fwpsk-fwpsinjecttransportreceiveasync0)
- [FwpsConstructIpHeaderForTransportPacket0](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/fwpsk/nf-fwpsk-fwpsconstructipheaderfortransportpacket0)
- [Microsoft WFPSampler](https://github.com/microsoft/Windows-driver-samples/tree/main/network/trans/WFPSampler)
