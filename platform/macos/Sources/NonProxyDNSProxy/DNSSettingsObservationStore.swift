import Foundation

final class DNSSettingsObservationStore: @unchecked Sendable {
    private let lock = NSLock()
    private var observation: NSKeyValueObservation?

    func replace(with value: NSKeyValueObservation) {
        let previous = lock.withLock {
            let current = observation
            observation = value
            return current
        }
        previous?.invalidate()
    }

    func invalidate() {
        let current = lock.withLock {
            let current = observation
            observation = nil
            return current
        }
        current?.invalidate()
    }
}
