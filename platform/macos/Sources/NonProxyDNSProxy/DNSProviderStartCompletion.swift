import Foundation

// NetworkExtension 回调未标记 Sendable；一次性容器将它安全地带过任务边界。
final class DNSProviderStartCompletion: @unchecked Sendable {
    private let lock = NSLock()
    private var handler: ((Error?) -> Void)?

    init(_ handler: @escaping (Error?) -> Void) {
        self.handler = handler
    }

    func complete(with error: Error?) {
        let callback = lock.withLock {
            let current = handler
            handler = nil
            return current
        }
        callback?(error)
    }
}
