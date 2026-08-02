import NetworkExtension
import Synchronization

final class RejectedFlowRegistry: Sendable {
    // Apple 声明 NEAppProxyFlow 为线程安全对象；字典本身由 Mutex 保护。
    private struct Entry: @unchecked Sendable {
        let flow: NEAppProxyFlow
        let errorCode: String
    }

    private let entries = Mutex([ObjectIdentifier: Entry]())

    var activeFlowCount: UInt64 {
        entries.withLock { UInt64($0.count) }
    }

    func reject(_ flow: NEAppProxyFlow, errorCode: String) {
        let identifier = ObjectIdentifier(flow)
        entries.withLock {
            $0[identifier] = Entry(flow: flow, errorCode: errorCode)
        }
        flow.open(withLocalFlowEndpoint: nil) { [weak self] openError in
            let entry = self?.entries.withLock {
                $0.removeValue(forKey: identifier)
            }
            guard openError == nil, let entry else {
                return
            }
            let error = NSError(
                domain: NEAppProxyErrorDomain,
                code: NEAppProxyFlowError.Code.refused.rawValue,
                userInfo: ["NonProxyErrorCode": entry.errorCode]
            )
            entry.flow.closeReadWithError(error)
            entry.flow.closeWriteWithError(error)
        }
    }

    func rejectAndHandle(_ flow: NEAppProxyFlow, errorCode: String) -> Bool {
        reject(flow, errorCode: errorCode)
        return true
    }

    func closeAll() {
        let retained = entries.withLock { entries -> [Entry] in
            let current = Array(entries.values)
            entries.removeAll(keepingCapacity: false)
            return current
        }
        let error = NEAppProxyFlowError(.aborted)
        for entry in retained {
            entry.flow.closeReadWithError(error)
            entry.flow.closeWriteWithError(error)
        }
    }
}
