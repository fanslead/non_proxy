# ADR-0038：远程订阅只通过固定公网地址的 HTTPS 获取

- 状态：已接受
- 日期：2026-08-01

## 背景

订阅 URL 经常把访问 Token 放在 path 或 query 中。让 `gatewayd` 直接使用通用 HTTP 客户端
会同时暴露环境代理继承、SSRF、DNS rebinding、重定向到私网、压缩炸弹、无界响应和 URL
日志泄露风险。远程订阅必须先有独立且可测试的获取边界，再接数据库、刷新计划和桌面 UI。

## 决策

1. 新增 `nonproxy-subscription`，只接受最长 4 KiB、无 userinfo/fragment/控制字符的
   `https://` URL。path、query 乃至 hostname 都可能含秘密，因此 endpoint 不实现可泄露
   URL 的 `Debug`，错误也只返回稳定 `NP_SUBSCRIPTION_*` 代码。
2. 每次获取先由系统 resolver 解析一次。结果为空，或任一答案属于 unspecified、loopback、
   RFC 1918、CGNAT、link-local、benchmark、documentation、multicast、reserved、IPv6 ULA
   等非公网范围时整次拒绝；不会在“一个公网、一个私网”的混合答案中挑公网继续。
3. 通过校验后只连接这组已解析的精确 `SocketAddr`，不再次按 hostname 拨号，从而把地址
   校验与实际连接绑定。允许映射到公网 IPv4 的 IPv4-mapped IPv6 和 well-known DNS64
   地址；映射到私网的同类地址继续拒绝。
4. 客户端直接创建 `TcpStream`，不读取 `HTTP_PROXY`、PAC 或系统代理设置。TLS 使用内置
   WebPKI 根和原始 hostname/IP 身份验证；证书错误不得降级。
5. HTTP/1.1 只发送 GET、固定 Accept/User-Agent 与 `Cache-Control: no-store`。只接受
   `200 OK`，不跟随重定向，不请求或接受压缩编码；`Content-Length` 和流式累计内容都受
   256 KiB 上限，整个解析、连接、TLS 和响应过程固定 15 秒超时。
6. 获取结果使用归零容器交给上层。该 crate 不解析节点、不写数据库、不保存 URL、不安排
   刷新，也不决定远程内容能否覆盖正在使用的出口。

## 后果

- 重定向型或仅内网可访问的订阅源当前会明确失败；后续若开放重定向，必须逐跳重新执行
  HTTPS、地址和响应策略，不能放宽现有入口。
- 保守拒绝混合 DNS 答案可能影响少数企业部署，但避免攻击者利用答案顺序进入本机、局域网
  或云 metadata 地址。企业内网订阅需要另行设计带明显授权和范围限制的模式。
- 获取核心可被 macOS 与 Windows 的同一 `gatewayd` 复用，平台 UI 不需要接触 DNS 或 TLS
  细节。

## 验收边界

仓库测试覆盖 URL 去敏、IPv4/IPv6 公网判定、私网/保留范围、DNS64 映射、WebPKI TLS
请求、Token 仅出现在请求目标、重定向、压缩和流式超限拒绝。它们不证明任一真实供应商
可访问，也不代表订阅源存储、预览确认、原子刷新或节点生命周期已经实现。
