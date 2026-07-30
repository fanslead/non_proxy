import Foundation
import NonProxyProviderCore

enum ProxyFlowCloseSender {
    static func send(
        on channel: ProxyFlowChannel,
        completion: @escaping @Sendable (Bool) -> Void
    ) {
        let result = channel.send(
            type: .close,
            requiresCredit: false,
            completion: completion
        )
        switch result {
        case .accepted:
            break
        case .insufficientCredit, .unavailable:
            completion(false)
        }
    }
}
