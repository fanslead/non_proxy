import Foundation
import Network
import NetworkExtension
import NonProxyProviderCore

// UDP 数据报的目标地址逐帧编码，所有队列状态由私有串行队列隔离。
final class ProxyUDPFlowRelay: FlowRelay, @unchecked Sendable {
    private static let maximumQueuedBytes = 256 * 1024
    private static let openTimeout: DispatchTimeInterval = .seconds(10)

    private let flow: NEAppProxyUDPFlow
    private let socketPath: String
    private let capability: Data
    private let proxyTarget: ProviderProxyTarget
    private let initialEndpoint: NPF1Endpoint
    private let budget: FlowRelayRegistry
    private let queue: DispatchQueue
    private let setupObserver: RelaySetupObserver
    private let onFinish: @Sendable (ProxyUDPFlowRelay) -> Void
    private var channel: ProxyFlowChannel?
    private var outgoing: [ProxyUDPOutgoingDatagram] = []
    private var outgoingBytes = 0
    private var incoming: [ProxyUDPIncomingDatagram] = []
    private var incomingBytes = 0
    private var isSending = false
    private var isWriting = false
    private var isAppClosing = false
    private var isOpening = false
    private var isOpened = false
    private var isChannelReady = false
    private var gatewayClosed = false
    private var isFinished = false
    private var finishError: Error?
    private var didNotifyFinish = false

    init(
        flow: NEAppProxyUDPFlow,
        socketPath: String,
        capability: Data,
        proxyTarget: ProviderProxyTarget,
        initialEndpoint: NPF1Endpoint,
        budget: FlowRelayRegistry,
        queue: DispatchQueue,
        onEstablished: @escaping @Sendable (String) -> Void,
        onSetupFailed: @escaping @Sendable (String) -> Void,
        onFinish: @escaping @Sendable (ProxyUDPFlowRelay) -> Void
    ) {
        self.flow = flow
        self.socketPath = socketPath
        self.capability = capability
        self.proxyTarget = proxyTarget
        self.initialEndpoint = initialEndpoint
        self.budget = budget
        self.queue = queue
        setupObserver = RelaySetupObserver(
            onEstablished: onEstablished, onFailed: onSetupFailed
        )
        self.onFinish = onFinish
    }

    func start() {
        queue.async { [weak self] in
            guard let self else {
                return
            }
            do {
                self.channel = try ProxyFlowChannel(
                    socketPath: self.socketPath,
                    capability: self.capability,
                    proxyTarget: self.proxyTarget,
                    endpoint: self.initialEndpoint,
                    openType: .openUDP,
                    queue: self.queue,
                    onEvent: { [weak self] event in
                        self?.handleChannelEvent(event)
                    }
                )
            } catch {
                self.finish(code: "NP_PROXY_CHANNEL_CONFIGURATION_INVALID")
                self.setupObserver.failed(code: "NP_PROXY_CHANNEL_CONFIGURATION_INVALID")
                return
            }
            self.channel?.start()
            self.queue.asyncAfter(deadline: .now() + Self.openTimeout) { [weak self] in
                guard let self, !self.isChannelReady, !self.isFinished else {
                    return
                }
                self.finish(code: "NP_PROXY_UDP_OPEN_TIMEOUT")
                self.setupObserver.failed(code: "NP_PROXY_UDP_OPEN_TIMEOUT")
            }
        }
    }

    func cancel() {
        queue.async { [weak self] in
            self?.finish(code: "NP_PROXY_RELAY_CANCELLED")
        }
    }
    private func openFlow() {
        guard !isOpening, !isOpened else {
            return
        }
        isOpening = true
        flow.open(withLocalFlowEndpoint: nil) { [weak self] error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                self.isOpening = false
                guard error == nil else {
                    self.isFinished = true
                    self.channel?.cancel()
                    self.notifyFinish()
                    return
                }
                self.isOpened = true
                if self.isFinished {
                    self.closeFlow(error: self.finishError)
                    self.notifyFinish()
                } else {
                    self.startReadingIfReady()
                }
            }
        }
    }

    private func handleChannelEvent(_ event: ProxyFlowChannelEvent) {
        guard !isFinished else {
            return
        }
        switch event {
        case .ready(let selectedOutboundID):
            isChannelReady = true
            setupObserver.established(selectedOutboundID: selectedOutboundID)
            openFlow()
            startReadingIfReady()
        case .creditAvailable:
            sendNextOutgoing()
        case .frame(let frame):
            handleFrame(frame)
        case .failed(let code):
            let reportSetupFailure = !isChannelReady
            finish(code: code)
            if reportSetupFailure {
                setupObserver.failed(code: code)
            }
        }
    }

    private func startReadingIfReady() {
        guard isOpened,
              isChannelReady,
              outgoing.isEmpty,
              !isSending,
              !isAppClosing,
              !isFinished
        else {
            return
        }
        readNextBatch()
    }

    private func readNextBatch() {
        flow.readDatagrams { [weak self] datagrams, error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                guard !self.isFinished else {
                    return
                }
                guard error == nil, let datagrams else {
                    self.finish(code: "NP_PROXY_UDP_APP_READ_FAILED")
                    return
                }
                guard !datagrams.isEmpty else {
                    self.closeFromApp()
                    return
                }
                do {
                    for (data, endpoint) in datagrams {
                        try self.enqueueOutgoing(data, endpoint: endpoint)
                    }
                    self.sendNextOutgoing()
                } catch {
                    self.finish(code: "NP_PROXY_UDP_QUEUE_LIMIT")
                }
            }
        }
    }

    private func enqueueOutgoing(
        _ data: Data,
        endpoint: NWEndpoint
    ) throws {
        let payload = try NPF1PayloadCodec.encodeDatagram(
            endpoint: ProxyFlowEndpointCodec.encode(endpoint: endpoint), content: data
        )
        guard outgoingBytes <= Self.maximumQueuedBytes - payload.count,
              budget.reserve(bytes: payload.count)
        else {
            throw NPF1ProtocolError.payloadTooLarge
        }
        outgoing.append(ProxyUDPOutgoingDatagram(payload: payload))
        outgoingBytes += payload.count
    }

    private func sendNextOutgoing() {
        guard !isSending,
              let next = outgoing.first,
              let channel,
              !isFinished
        else {
            return
        }
        isSending = true
        let result = channel.send(
            type: .datagram,
            payload: next.payload,
            requiresCredit: true,
            completion: { [weak self] sent in
                guard let self, !self.isFinished else {
                    return
                }
                let completed = self.outgoing.isEmpty
                    ? nil
                    : self.outgoing.removeFirst()
                self.isSending = false
                if let completed {
                    self.outgoingBytes = max(
                        0,
                        self.outgoingBytes - completed.payload.count
                    )
                    self.budget.release(bytes: completed.payload.count)
                }
                if sent {
                    self.sendNextOutgoing()
                    self.startReadingIfReady()
                }
            }
        )
        switch result {
        case .accepted:
            break
        case .insufficientCredit:
            isSending = false
        case .unavailable:
            isSending = false
            finish(code: "NP_PROXY_CHANNEL_UNAVAILABLE")
        }
    }

    private func handleFrame(_ frame: NPF1Frame) {
        switch frame.type {
        case .datagram:
            do {
                let decoded = try NPF1PayloadCodec.decodeDatagram(frame.payload)
                try enqueueIncoming(
                    data: decoded.content,
                    endpoint: ProxyFlowEndpointCodec.decode(decoded.endpoint),
                    acknowledgedBytes: frame.payload.count
                )
            } catch {
                finish(code: "NP_PROXY_UDP_RESPONSE_INVALID")
            }
        case .close:
            gatewayClosed = true
            finishAfterIncomingWrites()
        case .error:
            finish(code: sanitizedGatewayCode(frame.payload))
        default:
            finish(code: "NP_PROXY_UDP_FRAME_INVALID")
        }
    }

    private func enqueueIncoming(
        data: Data,
        endpoint: NWEndpoint,
        acknowledgedBytes: Int
    ) throws {
        guard incomingBytes <= Self.maximumQueuedBytes - data.count,
              budget.reserve(bytes: data.count)
        else {
            throw NPF1ProtocolError.payloadTooLarge
        }
        incoming.append(ProxyUDPIncomingDatagram(
            data: data, endpoint: endpoint, acknowledgedBytes: acknowledgedBytes
        ))
        incomingBytes += data.count
        writeNextIncoming()
    }

    private func writeNextIncoming() {
        guard !isWriting,
              let next = incoming.first,
              !isFinished
        else {
            return
        }
        isWriting = true
        flow.writeDatagrams([(next.data, next.endpoint)]) { [weak self] error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                guard !self.isFinished else {
                    return
                }
                let written = self.incoming.isEmpty
                    ? nil
                    : self.incoming.removeFirst()
                self.isWriting = false
                if let written {
                    self.incomingBytes = max(0, self.incomingBytes - written.data.count)
                    self.budget.release(bytes: written.data.count)
                    if error == nil {
                        self.channel?.acknowledgeReceived(bytes: written.acknowledgedBytes)
                    }
                }
                if error != nil {
                    self.finish(code: "NP_PROXY_UDP_APP_WRITE_FAILED")
                } else {
                    self.writeNextIncoming()
                    self.finishAfterIncomingWrites()
                }
            }
        }
    }

    private func finishAfterIncomingWrites() {
        guard gatewayClosed, !isWriting, incoming.isEmpty else {
            return
        }
        finish(error: nil)
    }

    private func finish(code: String) {
        finish(error: FlowRelayError.abort(code))
    }

    private func closeFromApp() {
        guard !isAppClosing, let channel else {
            finish(code: "NP_PROXY_CHANNEL_UNAVAILABLE")
            return
        }
        isAppClosing = true
        ProxyFlowCloseSender.send(on: channel) { [weak self] sent in
            guard let self, !self.isFinished else {
                return
            }
            if sent {
                self.finish(error: nil)
            } else {
                self.finish(code: "NP_PROXY_CLOSE_FAILED")
            }
        }
    }

    private func finish(error: Error?) {
        guard !isFinished else {
            return
        }
        isFinished = true
        finishError = error
        budget.release(bytes: outgoingBytes)
        outgoingBytes = 0
        outgoing.removeAll(keepingCapacity: false)
        budget.release(bytes: incomingBytes)
        incomingBytes = 0
        incoming.removeAll(keepingCapacity: false)
        isSending = false
        isWriting = false
        channel?.cancel()
        channel = nil
        if isOpened {
            closeFlow(error: error)
            notifyFinish()
        } else if !isOpening {
            if isChannelReady {
                openFlow()
            } else {
                notifyFinish()
            }
        }
    }

    private func closeFlow(error: Error?) {
        flow.closeReadWithError(error)
        flow.closeWriteWithError(error)
    }

    private func notifyFinish() {
        guard !didNotifyFinish else {
            return
        }
        didNotifyFinish = true
        onFinish(self)
    }
}
