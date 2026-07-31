import NetworkExtension

// NEAppProxyFlow 由单条 relay 串行队列拥有；包装仅用于把同一 flow 转交给
// 代理建立失败后的 DIRECT relay，不允许并发读写。
final class AppProxyFlowReference: @unchecked Sendable {
    let flow: NEAppProxyFlow

    init(_ flow: NEAppProxyFlow) {
        self.flow = flow
    }
}
