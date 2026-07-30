import Foundation

// NetworkExtension 的 Objective-C 回调尚未标注 Sendable；通过一次性容器限定跨任务边界。
final class ProviderStartCompletion: @unchecked Sendable {
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
