import Synchronization

final class RelaySetupObserver: Sendable {
    private let reported = Mutex(false)
    private let onEstablished: @Sendable (String) -> Void
    private let onFailed: @Sendable (String) -> Void

    init(
        onEstablished: @escaping @Sendable (String) -> Void,
        onFailed: @escaping @Sendable (String) -> Void
    ) {
        self.onEstablished = onEstablished
        self.onFailed = onFailed
    }

    func established(selectedOutboundID: String) {
        guard claimOutcome() else {
            return
        }
        onEstablished(selectedOutboundID)
    }

    func failed(code: String) {
        guard claimOutcome() else {
            return
        }
        onFailed(code)
    }

    private func claimOutcome() -> Bool {
        reported.withLock {
            guard !$0 else {
                return false
            }
            $0 = true
            return true
        }
    }
}
