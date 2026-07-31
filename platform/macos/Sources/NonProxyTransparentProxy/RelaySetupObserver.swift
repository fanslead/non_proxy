import Synchronization

final class RelaySetupObserver: Sendable {
    private let reported = Mutex(false)
    private let onEstablished: @Sendable () -> Void
    private let onFailed: @Sendable (String) -> Void

    init(
        onEstablished: @escaping @Sendable () -> Void,
        onFailed: @escaping @Sendable (String) -> Void
    ) {
        self.onEstablished = onEstablished
        self.onFailed = onFailed
    }

    func established() {
        guard claimOutcome() else {
            return
        }
        onEstablished()
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
