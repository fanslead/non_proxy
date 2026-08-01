# nonproxy-subscription

该 crate 只负责把一个已明确授权的 HTTPS 订阅地址安全获取为有界字节内容：解析秘密地址、
阻断本地/私网目标、固定 DNS 解析结果、验证 WebPKI TLS、执行最小 HTTP/1.1 GET，并限制
超时、状态、编码与响应大小。

它不解析代理节点，不访问 SQLite 或系统凭据库，不管理刷新计划，也不决定订阅内容是否可
覆盖现有出口。上述职责分别属于 `gatewayd` 编排、订阅存储和出口导入模块。

生产调用只使用内置 WebPKI 根、直连 `TcpStream` 和解析后 `SocketAddr`，不会读取系统代理
环境变量。endpoint 的 path/query 视为秘密；`Debug`、错误类型和公开 API 不返回 URL。
