import Foundation

public typealias MacBridgeCallback = @convention(c) (
    UInt64,
    Int32,
    Int32,
    UnsafePointer<UInt8>?,
    Int,
    UnsafeMutableRawPointer?
) -> Void

struct BridgeCallbackSink: @unchecked Sendable {
    private let operationID: UInt64
    private let callback: MacBridgeCallback
    private let context: UnsafeMutableRawPointer?

    init(
        operationID: UInt64,
        callback: @escaping MacBridgeCallback,
        context: UnsafeMutableRawPointer?
    ) {
        self.operationID = operationID
        self.callback = callback
        self.context = context
    }

    func progress(_ payload: BridgeEventPayload) {
        emit(eventKind: 1, statusCode: 1, value: payload)
    }

    func complete(_ payload: BridgeEventPayload) {
        emit(
            eventKind: 2,
            statusCode: payload.success ? 0 : -1,
            value: payload
        )
    }

    func completeProbe(_ payload: ProbePayload) {
        emit(eventKind: 2, statusCode: 0, value: payload)
    }

    private func emit<Value: Encodable>(
        eventKind: Int32,
        statusCode: Int32,
        value: Value
    ) {
        let data: Data
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            data = try encoder.encode(value)
        } catch {
            data = Data(
                #"{"errorCode":"NP_MAC_JSON_ENCODE_FAILED","message":"原生桥接无法编码响应。","success":false}"#
                    .utf8
            )
        }

        data.withUnsafeBytes { bytes in
            let pointer = bytes.bindMemory(to: UInt8.self).baseAddress
            callback(
                operationID,
                eventKind,
                statusCode,
                pointer,
                data.count,
                context
            )
        }
    }
}

final class BridgeOperationGate: @unchecked Sendable {
    static let shared = BridgeOperationGate()

    private let lock = NSLock()
    private var operationActive = false

    private init() {}

    func begin() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !operationActive else {
            return false
        }
        operationActive = true
        return true
    }

    func end() {
        lock.lock()
        operationActive = false
        lock.unlock()
    }
}
