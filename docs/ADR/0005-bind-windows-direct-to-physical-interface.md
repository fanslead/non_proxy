# ADR-0005：Windows DIRECT 动态绑定物理接口

- 状态：Accepted
- 日期：2026-07-31
- 决策范围：Windows DIRECT TCP 与 DNS 出口选择

## 背景

把 WFP 重定向连接在用户态重新 `connect`，如果仍使用系统默认路由，第三方 VPN 的默认路由可能再次成为出口。这只能证明 NonProxy 做出了 DIRECT 决策，不能证明流量离开了 VPN。

Windows 为 IPv4/IPv6 unicast socket 提供按接口选择的 `IP_UNICAST_IF` 和 `IPV6_UNICAST_IF`。接口索引会随网卡禁用、启用等状态变化而改变，不能持久化为长期配置。默认路由的实际优先级还必须使用“路由 metric 偏移 + 接口 metric”，不能只看其中一个值。

## 决策

新增独立的 `nonproxy-windows-network` 平台 crate：

- 从 `GetIfTable2` 读取接口状态，不通过名称包含 `VPN`、`TAP` 等脆弱字符串猜测。
- 只保留 operational、hardware、media connected、非 filter、非 endpoint 的接口，并排除 PPP、loopback 和 tunnel 类型。
- 分别读取 IPv4/IPv6 默认路由；通过 `GetIpInterfaceEntry` 验证接口仍 connected、没有禁用默认路由，并将接口 metric 加入总 metric。
- 对 IPv4/IPv6 分别选择物理出口。优先有物理 connector 的接口，其后依次比较总 metric、链路速度和稳定的接口索引顺序。
- 结果只缓存一秒，连接建立时重新确认，避免把易变接口索引写入数据库或策略快照。
- IPv4 按官方要求把接口索引转换为网络字节序后设置 `IP_UNICAST_IF`；IPv6 使用主机字节序设置 `IPV6_UNICAST_IF`。

Windows TCP 数据面使用两个彼此独立的 dialer：

- DIRECT 及代理失败后的 fail-open DIRECT：传递原 WFP redirect records，并绑定当前物理接口。
- PROXY 控制连接：只传递 redirect records，不强制绑定物理接口，使所选代理或 VPN 出口仍可按其系统路径工作。

Windows DIRECT DNS UDP/TCP 复用同一个 socket 绑定实现。没有对应地址族的可信物理接口时，DIRECT 明确失败，不静默退回可能受 VPN 接管的默认路由。

## 安全与能力边界

- 接口枚举和 socket 选项只存在于 Windows 平台 crate，策略核心不依赖 Win32 类型。
- 选择器接受纯数据并具备跨平台单元测试；Win32 FFI 继续通过 x64/ARM64 交叉编译门禁。
- 物理接口绑定不能绕过 MDM、Always-On VPN、VPN 的 WFP callout、`DisableDefaultRoutes` 或其他强制策略。
- 本批次不等于 Windows DNS 接管、域名关联、UDP/QUIC 或真实 VPN 共存验收完成。
- 完成声明仍需要在真实 Windows 上采集接口、路由、外网出口 IP、WFP 状态和第三方 VPN 对照证据。

## 参考

- [IPPROTO_IP socket options](https://learn.microsoft.com/en-us/windows/win32/winsock/ipproto-ip-socket-options)
- [IPPROTO_IPV6 socket options](https://learn.microsoft.com/en-us/windows/win32/winsock/ipproto-ipv6-socket-options)
- [GetIfTable2 / MIB_IF_ROW2](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/ns-netioapi-mib_if_row2)
- [GetIpInterfaceEntry](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-getipinterfaceentry)
- [MIB_IPINTERFACE_ROW metric](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/ns-netioapi-mib_ipinterface_row)
- [GetIpForwardTable2](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-getipforwardtable2)
