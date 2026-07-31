# ADR-0004：使用最小 WFP Connect Redirect Callout

- 状态：Accepted
- 日期：2026-07-31
- 决策范围：Windows TCP、明文 DNS 捕获与本地重定向

## 背景

NonProxy 必须在第三方 VPN 已开启时，先于其代理路径取得连接决策权。仅使用用户态 WFP 管理 API可以添加 filter 和读取策略对象，但不能修改 ALE connect tuple；本地代理重定向需要内核 callout 在 `FWPM_LAYER_ALE_CONNECT_REDIRECT_V4/V6` 修改可写层数据。

Microsoft 的 WFP 指南要求：标准过滤足够时优先使用用户态管理；需要 connect redirection 时使用 callout，并在本地代理的 accepted/outbound socket 间传递 redirect context/records，以避免重定向循环和保持多个代理的兼容性。

## 决策

增加一个最小 WDM Driver：

- 只注册 IPv4/IPv6 ALE Connect Redirect callout。
- 只接受版本化定长 IOCTL 配置：generation、Service PID、IPv4/IPv6 TCP/DNS
  loopback 端口，以及相互独立的 DNS/TCP enable flag。
- DNS flag 只重定向远端 53 端口的 TCP/UDP；TCP flag 重定向其余 TCP。
  Service 自身 PID、自身已经重定向的连接和未启用状态直接放行。
- context 只包含原始本地/远端 `SOCKADDR_STORAGE`、PID 和最多 4096 字节 ALE App ID。
- 不在内核执行产品策略。App/域名/CIDR/端口匹配、DIRECT/PROXY/BLOCK、出口协议、凭据和数据库全部留在 `gatewayd`。
- 内部分配、可写层获取或应用失败时 fail-open，并增加有界状态计数；控制 handle cleanup 必须立即禁用重定向。

用户态 Service：

- 先绑定 TCP 与 DNS 的 IPv4/IPv6 listener，打开 disabled 驱动。
- 在动态 BFE transaction 中添加 provider、sublayer、V4/V6 callout，以及高
  优先级 TCP/UDP 53 filter 和普通 TCP filter。
- 先下发 DNS-only 配置。没有活动快照时，本地 DNS 对普通查询明确走物理
  DIRECT，仅用随机 `.invalid` 查询验证 WFP DNS 路径，不能先接管普通 TCP。
- 只有系统 resolver 探针真实命中本地 listener，且哈希验证通过的活动快照
  已加载时，才同时启用普通 TCP，并把 DNS/WFP Provider 上报为 `Ready`。
- DNS 探针失效时只关闭普通 TCP；保留 DNS-only 路径以便恢复探针。
- 对 accepted socket 查询 redirect records/context；每个直接或代理出口 socket 在 connect 前设置同一 records。
- 停止时先下发全 disabled，再停止 listener；BFE engine handle 关闭自动删除
  动态对象。控制 handle 异常关闭也会清零所有 flag。

本 ADR 描述 TCP 与明文 DNS 的 connect redirect 子系统。同一最小 Driver 后续
增加的 UDP flow identity、数据报搬运和反向注入由
[ADR-0008](0008-divert-windows-udp-datagrams.md) 单独约束；这不改变“内核不
执行产品策略”的边界。

## 安全边界

- 控制设备 exclusive open，`IoCreateDeviceSecure` 只授权 SYSTEM 和 Administrators。
- 命名管道仍使用安装器下发的独立 SDDL；UI 不能直接打开 Driver。
- Driver 不接受 JSON/Protobuf/正则/域名/订阅，不读取文件或注册表，不发网络请求。
- App ID 和 redirect records 有硬上限，不记录请求正文、Cookie、TLS 明文或凭据。
- 固定 GUID 和 ABI 同时在 C/Rust 中验证；ABI 变更必须升级版本。

## 未覆盖范围

- 远端 53 端口之外的 UDP/QUIC 不复用本 ADR 的 connect redirect，使用
  ADR-0008 的独立数据报搬运；真实 Windows 验收完成前仍不能表述为可用。
- 网站规则需要 Windows DNS 捕获和应用归属关联，不能把 TLS SNI 当唯一域名来源。
- 打包应用身份、签名验证、生产签名、安装/升级回滚、Driver Verifier 和 VPN 共存路径必须在真实 Windows 环境验收。

## 参考

- [Callout Driver Programming Considerations](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/callout-driver-programming-considerations)
- [Roadmap for Developing WFP Callout Drivers](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/roadmap-for-developing-wfp-callout-drivers)
- [Using Bind or Connect Redirection](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/using-bind-or-connect-redirection)
- [SIO_SET_WFP_CONNECTION_REDIRECT_RECORDS](https://learn.microsoft.com/en-us/windows/win32/winsock/sio-set-wfp-connection-redirect-records)
- [Microsoft WFPSampler](https://learn.microsoft.com/en-us/samples/microsoft/windows-driver-samples/windows-filtering-platform-sample/)
