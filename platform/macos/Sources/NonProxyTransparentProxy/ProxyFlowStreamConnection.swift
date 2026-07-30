import Foundation
import Network

protocol ProxyFlowStreamConnection: AnyObject, Sendable {
    var stateUpdateHandler: (@Sendable (NWConnection.State) -> Void)? {
        get set
    }

    func flowStart(queue: DispatchQueue)

    func flowSend(
        content: Data?,
        completion: @escaping @Sendable (NWError?) -> Void
    )

    func flowReceive(
        minimumIncompleteLength: Int,
        maximumLength: Int,
        completion: @escaping @Sendable (
            Data?,
            NWConnection.ContentContext?,
            Bool,
            NWError?
        ) -> Void
    )

    func flowCancel()
}

extension NWConnection: ProxyFlowStreamConnection {
    func flowStart(queue: DispatchQueue) {
        start(queue: queue)
    }

    func flowSend(
        content: Data?,
        completion: @escaping @Sendable (NWError?) -> Void
    ) {
        send(
            content: content,
            completion: .contentProcessed(completion)
        )
    }

    func flowReceive(
        minimumIncompleteLength: Int,
        maximumLength: Int,
        completion: @escaping @Sendable (
            Data?,
            NWConnection.ContentContext?,
            Bool,
            NWError?
        ) -> Void
    ) {
        receive(
            minimumIncompleteLength: minimumIncompleteLength,
            maximumLength: maximumLength,
            completion: completion
        )
    }

    func flowCancel() {
        cancel()
    }
}

typealias ProxyFlowConnectionFactory =
    @Sendable (String) -> any ProxyFlowStreamConnection

enum LiveProxyFlowConnectionFactory {
    static func make(socketPath: String) -> any ProxyFlowStreamConnection {
        NWConnection(
            to: .unix(path: socketPath),
            using: .tcp
        )
    }
}
