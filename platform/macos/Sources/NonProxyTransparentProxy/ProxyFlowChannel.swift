import Foundation
import Network
import NonProxyProviderCore

enum ProxyFlowChannelEvent: Sendable {
    case ready(selectedOutboundID: String)
    case creditAvailable
    case frame(NPF1Frame)
    case failed(String)
}

enum ProxyFlowChannelSendResult {
    case accepted
    case insufficientCredit
    case unavailable
}

// 所有方法和回调都在单条 flow 的私有串行队列执行。
final class ProxyFlowChannel: @unchecked Sendable {
    private static let initialWindowBytes: UInt32 = 256 * 1024
    private static let maximumWindowBytes = UInt64(
        NPF1PayloadCodec.maximumWindowBytes
    )
    private static let maximumReceiveBytes = 64 * 1024
    private static let maximumWriteQueueFrames = 64
    private static let establishmentTimeout: DispatchTimeInterval = .seconds(15)

    private struct PendingWrite {
        let data: Data
        let completion: @Sendable (Bool) -> Void
    }

    private let connection: any ProxyFlowStreamConnection
    private var capability: Data
    private let proxyTarget: ProviderProxyTarget
    private let endpoint: NPF1Endpoint
    private let openType: NPF1FrameType
    private let flowID: NPF1FlowID
    private let queue: DispatchQueue
    private let onEvent: @Sendable (ProxyFlowChannelEvent) -> Void
    private var decoder = NPF1FrameDecoder()
    private var inboundSequence = NPF1SequenceTracker()
    private var outboundSequence: UInt64 = 0
    private var sendCredit: UInt64 = 0
    private var receiveCredit = UInt64(initialWindowBytes)
    private var writes: [PendingWrite] = []
    private var isWriting = false
    private var isConnectionReady = false
    private var isProtocolReady = false
    private var isClosed = false

    init(
        socketPath: String,
        capability: Data,
        proxyTarget: ProviderProxyTarget,
        endpoint: NPF1Endpoint,
        openType: NPF1FrameType,
        queue: DispatchQueue,
        connectionFactory: ProxyFlowConnectionFactory =
            LiveProxyFlowConnectionFactory.make,
        onEvent: @escaping @Sendable (ProxyFlowChannelEvent) -> Void
    ) throws {
        guard socketPath.hasPrefix("/"),
              !socketPath.contains("\0"),
              capability.count == NPF1PayloadCodec.capabilityBytes,
              proxyTarget.isValid,
              matchesOpenType(openType)
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        connection = connectionFactory(socketPath)
        self.capability = capability
        self.proxyTarget = proxyTarget
        self.endpoint = endpoint
        self.openType = openType
        flowID = try NPF1FlowID.random()
        self.queue = queue
        self.onEvent = onEvent
    }

    func start() {
        guard !isClosed else {
            return
        }
        connection.stateUpdateHandler = { [weak self] state in
            self?.handleConnectionState(state)
        }
        connection.flowStart(queue: queue)
        queue.asyncAfter(
            deadline: .now() + Self.establishmentTimeout
        ) { [weak self] in
            guard let self, !self.isProtocolReady, !self.isClosed else {
                return
            }
            self.fail(code: "NP_PROXY_CHANNEL_CONNECT_TIMEOUT")
        }
    }

    func send(
        type: NPF1FrameType,
        payload: Data = Data(),
        requiresCredit: Bool,
        completion: @escaping @Sendable (Bool) -> Void = { _ in }
    ) -> ProxyFlowChannelSendResult {
        guard isProtocolReady, !isClosed else {
            return .unavailable
        }
        let payloadBytes = UInt64(payload.count)
        if requiresCredit, payloadBytes > sendCredit {
            return .insufficientCredit
        }
        if requiresCredit {
            sendCredit -= payloadBytes
        }
        do {
            try enqueue(type: type, payload: payload, completion: completion)
            return .accepted
        } catch {
            if requiresCredit {
                sendCredit += payloadBytes
            }
            fail(code: "NP_PROXY_CHANNEL_WRITE_QUEUE_FAILED")
            return .unavailable
        }
    }

    func acknowledgeReceived(bytes: Int) {
        guard bytes > 0, !isClosed,
              let increment = UInt32(exactly: bytes),
              receiveCredit <= Self.maximumWindowBytes - UInt64(increment)
        else {
            fail(code: "NP_PROXY_CHANNEL_WINDOW_INVALID")
            return
        }
        do {
            receiveCredit += UInt64(increment)
            try enqueue(
                type: .windowUpdate,
                payload: NPF1PayloadCodec.encodeWindowUpdate(increment)
            )
        } catch {
            fail(code: "NP_PROXY_CHANNEL_WINDOW_WRITE_FAILED")
        }
    }

    func cancel() {
        close()
    }

    private func handleConnectionState(_ state: NWConnection.State) {
        guard !isClosed else {
            return
        }
        switch state {
        case .ready where !isConnectionReady:
            isConnectionReady = true
            sendOpen()
        case .failed:
            fail(code: "NP_PROXY_CHANNEL_CONNECT_FAILED")
        case .cancelled:
            if !isClosed {
                fail(code: "NP_PROXY_CHANNEL_CLOSED")
            }
        default:
            break
        }
    }

    private func sendOpen() {
        do {
            let payload = try NPF1PayloadCodec.encodeOpen(
                capability: capability,
                proxyTarget: proxyTarget,
                endpoint: endpoint,
                initialWindowBytes: Self.initialWindowBytes
            )
            capability = Data()
            try enqueue(type: openType, payload: payload) { [weak self] sent in
                guard let self else {
                    return
                }
                if sent {
                    self.receiveNext()
                }
            }
        } catch {
            fail(code: "NP_PROXY_CHANNEL_OPEN_FAILED")
        }
    }

    private func enqueue(
        type: NPF1FrameType,
        payload: Data,
        completion: @escaping @Sendable (Bool) -> Void = { _ in }
    ) throws {
        guard !isClosed,
              writes.count < Self.maximumWriteQueueFrames,
              outboundSequence < UInt64.max
        else {
            throw NPF1ProtocolError.sequenceExhausted
        }
        let frame = try NPF1Frame(
            type: type,
            flowID: flowID,
            sequence: outboundSequence,
            payload: payload
        )
        outboundSequence += 1
        writes.append(
            PendingWrite(
                data: try NPF1FrameCodec.encode(frame),
                completion: completion
            )
        )
        writeNext()
    }

    private func writeNext() {
        guard !isWriting, let next = writes.first, !isClosed else {
            return
        }
        isWriting = true
        connection.flowSend(
            content: next.data
        ) { [weak self] error in
            guard let self else {
                return
            }
            let completed = self.writes.isEmpty
                ? nil
                : self.writes.removeFirst()
            self.isWriting = false
            completed?.completion(error == nil)
            if error != nil {
                self.fail(code: "NP_PROXY_CHANNEL_WRITE_FAILED")
            } else {
                self.writeNext()
            }
        }
    }

    private func receiveNext() {
        guard !isClosed else {
            return
        }
        connection.flowReceive(
            minimumIncompleteLength: 1,
            maximumLength: Self.maximumReceiveBytes
        ) { [weak self] data, _, isComplete, error in
            guard let self, !self.isClosed else {
                return
            }
            if error != nil {
                self.fail(code: "NP_PROXY_CHANNEL_READ_FAILED")
                return
            }
            do {
                if let data, !data.isEmpty {
                    for frame in try self.decoder.append(data) {
                        try self.handle(frame)
                    }
                }
            } catch {
                self.fail(code: "NP_PROXY_CHANNEL_PROTOCOL_INVALID")
                return
            }
            if isComplete {
                self.fail(code: "NP_PROXY_CHANNEL_CLOSED")
            } else {
                self.receiveNext()
            }
        }
    }

    private func handle(_ frame: NPF1Frame) throws {
        guard frame.flowID == flowID else {
            throw NPF1ProtocolError.invalidFlowID
        }
        try inboundSequence.accept(frame.sequence)
        switch frame.type {
        case .windowUpdate:
            let increment = try NPF1PayloadCodec.decodeWindowUpdate(
                frame.payload
            )
            if isProtocolReady {
                try addSendCredit(increment)
                onEvent(.creditAvailable)
            } else {
                guard case .outbound(let outboundID) = proxyTarget else {
                    throw NPF1ProtocolError.invalidPayload
                }
                try addSendCredit(increment)
                isProtocolReady = true
                onEvent(.ready(selectedOutboundID: outboundID))
            }
        case .ready:
            guard !isProtocolReady,
                  case .group(_, _, let memberIDs) = proxyTarget
            else {
                throw NPF1ProtocolError.invalidPayload
            }
            let ready = try NPF1PayloadCodec.decodeReady(frame.payload)
            guard memberIDs.contains(ready.selectedOutboundID) else {
                throw NPF1ProtocolError.invalidPayload
            }
            try addSendCredit(ready.initialWindowBytes)
            isProtocolReady = true
            onEvent(.ready(selectedOutboundID: ready.selectedOutboundID))
        case .data, .datagram:
            guard isProtocolReady,
                  !frame.payload.isEmpty,
                  UInt64(frame.payload.count) <= receiveCredit
            else {
                throw NPF1ProtocolError.invalidPayload
            }
            receiveCredit -= UInt64(frame.payload.count)
            onEvent(.frame(frame))
        case .halfClose, .close:
            guard isProtocolReady else {
                throw NPF1ProtocolError.invalidPayload
            }
            onEvent(.frame(frame))
        case .error where !isProtocolReady:
            fail(code: NPF1PayloadCodec.decodeErrorCode(frame.payload))
        case .error:
            onEvent(.frame(frame))
        case .ping:
            try enqueue(type: .pong, payload: Data())
        case .pong:
            break
        case .openTCP, .openUDP:
            throw NPF1ProtocolError.invalidFrameType
        }
    }

    private func addSendCredit(_ increment: UInt32) throws {
        guard sendCredit <= Self.maximumWindowBytes - UInt64(increment)
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        sendCredit += UInt64(increment)
    }

    private func fail(code: String) {
        guard !isClosed else {
            return
        }
        close()
        onEvent(.failed(code))
    }

    private func close() {
        guard !isClosed else {
            return
        }
        isClosed = true
        connection.stateUpdateHandler = nil
        connection.flowCancel()
        let completions = writes.map(\.completion)
        writes.removeAll(keepingCapacity: false)
        isWriting = false
        completions.forEach { $0(false) }
    }
}

private func matchesOpenType(_ value: NPF1FrameType) -> Bool {
    value == .openTCP || value == .openUDP
}
