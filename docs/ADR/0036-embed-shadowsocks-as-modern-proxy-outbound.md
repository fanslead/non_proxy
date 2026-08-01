# ADR-0036：嵌入 Shadowsocks 作为首个现代代理出口

- 状态：已接受
- 日期：2026-08-01

## 背景

SOCKS5 和 HTTP CONNECT 只能复用已经存在的本地代理端口，不能让普通用户直接导入
常见现代代理节点。NonProxy 需要一个许可证清晰、可嵌入共享 Rust 数据面的协议实现，
同时必须避免“能解析分享链接”却无法可靠认证密钥或承载 UDP 的半成品状态。

Shadowsocks TCP 客户端在创建加密流时主要向服务端写入，错误方法或密钥不一定立即在
本地连接阶段报错。若沿用只建立流的健康检查，可能把不可用节点错误标记为 `READY`，
进而允许其成为 fail-closed 默认出口。

## 决策

1. 使用 MIT 许可的
   [`shadowsocks` 1.24.0](https://github.com/shadowsocks/shadowsocks-rust) 和其 MIT
   `shadowsocks-crypto` 依赖。关闭默认 feature，只启用 AEAD、AEAD-2022 与 replay
   attack detection；版本和完整许可证文本进入 `Cargo.lock` 与 `third_party/`。
2. 首版只接受 `aes-128-gcm`、`aes-256-gcm`、`chacha20-poly1305`、
   `2022-blake3-aes-128-gcm`、`2022-blake3-aes-256-gcm` 和
   `2022-blake3-chacha20-poly1305`。明确拒绝 `none`、旧式流加密和库中其他方法；
   AEAD-2022 密钥继续由上游 `ServerConfig` 校验 base64 形态与长度。
3. `proxy-uri-list-v1` 新增 `ss://`：接受 SIP002 base64 userinfo、百分号编码的
   `method:password` userinfo，以及旧式整段 base64 `method:password@host:port`。
   Shadowsocks 必须显式端口；首版拒绝 path、query 和 SIP003 plugin。错误只包含稳定
   错误码与行号，不回显原始链接、方法或密钥。
4. JSON 导入使用 `kind=shadowsocks`、`method` 和 `password`。方法与密钥编码成版本化
   secret blob，只写 macOS Keychain、Windows Credential Manager 或对应安全存储；
   SQLite、审计事件、RPC summary、桌面预览和日志只保存 Password 类型的引用与去敏标签。
   临时配置和 secret 缓冲区沿用归零及补偿事务。
5. `nonproxy-outbound` 实现 TCP stream 和保持逐数据报目标地址的 UDP session，统一复用
   NPF1 TCP/UDP 数据通道。连接器声明 TCP、UDP、IPv4、IPv6，并可在通过当前 revision
   的新鲜认证测试后成为默认出口。
6. Shadowsocks 健康检查固定连接 `example.com:443`，再通过该加密流使用内置 WebPKI
   根完成 TLS 握手。只有 TLS 成功才记录 `READY`；失败使用
   `NP_FLOW_OUTBOUND_AUTHENTICATION_FAILED`，且不透出 rustls、协议库或服务端内部错误。
   探测不发送 HTTP 请求、Cookie 或用户业务载荷。HTTP CONNECT 与 SOCKS5 保持原有
   代理握手语义。
7. 桌面端显示明确的 `Shadowsocks` 协议标签、`ss://` 粘贴提示、默认路由和签名出口
   操作；不把 Shadowsocks 塞入只面向本地 SOCKS5/HTTP 监听端口的手动表单。
8. Base64 Shadowsocks 订阅内容的离线导入由 ADR-0037 追加；本决策不实现订阅源 URL、
   远程刷新、SIP003 plugin、VMess/VLESS、Trojan、Hysteria 2、TUIC、WireGuard、
   OpenVPN、OpenConnect 或与任意第三方 VPN 同时运行的兼容保证。

## 后果

- 普通用户可以直接粘贴常见 Shadowsocks 分享链接，无需识别节点请求域名或理解内部
  代理规则。
- 错误密钥不能只因 TCP 已建立就通过默认路由门禁；认证测试仍不等同于签名公网出口、
  实际策略命中、UDP/QUIC 或真实设备路径验收。
- 协议实现位于独立 outbound 模块，存储、控制契约和桌面 UI 只依赖稳定类型与能力，
  后续 Windows 数据面无需复制协议核心。
- 增加的密码学与网络依赖必须继续接受依赖升级、许可证清单、漏洞和互操作性审查。

## 验收边界

仓库测试覆盖六种允许方法、非法方法/密钥、TCP/UDP 本地互操作、分享链接形态、秘密
隔离、默认路由能力、控制契约和桌面映射。它们不代替正式签名 macOS System Extension、
Windows WFP/WDK、真实 Shadowsocks 服务端、IPv4/IPv6 公网、UDP/QUIC、睡眠漫游和
第三方 VPN 共存矩阵验收。
