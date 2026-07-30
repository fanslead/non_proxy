import Foundation
import NetworkExtension
import NonProxyProviderCore

final class ProxyTCPFlowRelay: FlowRelay, @unchecked Sendable {
    private static let maximumAppReadBytes = NPF1FrameCodec.maximumPayloadBytes
    private static let openTimeout: DispatchTimeInterval = .seconds(10)

    private let flow: NEAppProxyTCPFlow
    private let socketPath: String
    private let capability: Data
    private let outboundID: String
    private let endpoint: NPF1Endpoint
    private let budget: FlowRelayRegistry
    private let queue: DispatchQueue
    private let onFinish: @Sendable (ProxyTCPFlowRelay) -> Void
    private var channel: ProxyFlowChannel?
    private var pendingAppData: Data?
    private var inboundWrites: [ProxyTCPInboundWrite] = []
    private var isWritingInbound = false
    private var isOpening = false
    private var isOpened = false
    private var isChannelReady = false
    private var appReadFinished = false
    private var remoteReadFinished = false
    private var gatewayClosed = false
    private var isFinished = false
    private var finishError: Error?
    private var didNotifyFinish = false

    init(
        flow: NEAppProxyTCPFlow,
        socketPath: String,
        capability: Data,
        outboundID: String,
        endpoint: NPF1Endpoint,
        budget: FlowRelayRegistry,
        queue: DispatchQueue,
        onFinish: @escaping @Sendable (ProxyTCPFlowRelay) -> Void
    ) {
        self.flow = flow
        self.socketPath = socketPath
        self.capability = capability
        self.outboundID = outboundID
        self.endpoint = endpoint
        self.budget = budget
        self.queue = queue
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
                    outboundID: self.outboundID,
                    endpoint: self.endpoint,
                    openType: .openTCP,
                    queue: self.queue,
                    onEvent: { [weak self] event in
                        self?.handleChannelEvent(event)
                    }
                )
            } catch {
                self.finish(code: "NP_PROXY_CHANNEL_CONFIGURATION_INVALID")
                return
            }
            self.openFlow()
            self.channel?.start()
            self.queue.asyncAfter(
                deadline: .now() + Self.openTimeout
            ) { [weak self] in
                guard let self, self.isOpening, !self.isFinished else {
                    return
                }
                self.finish(code: "NP_PROXY_TCP_OPEN_TIMEOUT")
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
                    self.startPumpsIfReady()
                }
            }
        }
    }

    private func handleChannelEvent(_ event: ProxyFlowChannelEvent) {
        guard !isFinished else {
            return
        }
        switch event {
        case .ready:
            isChannelReady = true
            startPumpsIfReady()
        case .creditAvailable:
            sendPendingAppData()
        case .frame(let frame):
            handleFrame(frame)
        case .failed(let code):
            finish(code: code)
        }
    }

    private func startPumpsIfReady() {
        guard isOpened, isChannelReady, !isFinished else {
            return
        }
        if pendingAppData == nil, !appReadFinished {
            readFromApp()
        }
    }

    private func readFromApp() {
        guard pendingAppData == nil, !appReadFinished, !isFinished else {
            return
        }
        flow.readData { [weak self] data, error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                self.handleAppData(data, error: error)
            }
        }
    }

    private func handleAppData(_ data: Data?, error: Error?) {
        guard !isFinished else {
            return
        }
        if error != nil {
            finish(code: "NP_PROXY_TCP_APP_READ_FAILED")
            return
        }
        guard let data, !data.isEmpty else {
            appReadFinished = true
            sendHalfClose()
            return
        }
        guard data.count <= Self.maximumAppReadBytes,
              budget.reserve(bytes: data.count)
        else {
            finish(code: "NP_PROXY_TCP_QUEUE_LIMIT")
            return
        }
        pendingAppData = data
        sendPendingAppData()
    }

    private func sendPendingAppData() {
        guard let data = pendingAppData,
              let channel,
              !isFinished
        else {
            return
        }
        let byteCount = data.count
        let result = channel.send(
            type: .data,
            payload: data,
            requiresCredit: true,
            completion: { [weak self] sent in
                guard let self, !self.isFinished else {
                    return
                }
                self.budget.release(bytes: byteCount)
                self.pendingAppData = nil
                if sent {
                    self.readFromApp()
                }
            }
        )
        guard !isFinished else {
            return
        }
        switch result {
        case .accepted, .insufficientCredit:
            break
        case .unavailable:
            budget.release(bytes: byteCount)
            pendingAppData = nil
            finish(code: "NP_PROXY_CHANNEL_UNAVAILABLE")
        }
    }

    private func sendHalfClose() {
        guard let channel else {
            finish(code: "NP_PROXY_CHANNEL_UNAVAILABLE")
            return
        }
        switch channel.send(
            type: .halfClose,
            requiresCredit: false
        ) {
        case .accepted:
            finishIfComplete()
        case .insufficientCredit, .unavailable:
            finish(code: "NP_PROXY_HALF_CLOSE_FAILED")
        }
    }

    private func handleFrame(_ frame: NPF1Frame) {
        switch frame.type {
        case .data:
            enqueueInbound(
                frame.payload,
                acknowledgedBytes: frame.payload.count
            )
        case .halfClose:
            remoteReadFinished = true
            finishRemoteReadIfPossible()
        case .close:
            gatewayClosed = true
            finishAfterInboundWrites()
        case .error:
            finish(code: sanitizedGatewayCode(frame.payload))
        default:
            finish(code: "NP_PROXY_TCP_FRAME_INVALID")
        }
    }

    private func enqueueInbound(
        _ data: Data,
        acknowledgedBytes: Int
    ) {
        guard !remoteReadFinished,
              !gatewayClosed,
              budget.reserve(bytes: data.count)
        else {
            finish(code: "NP_PROXY_TCP_RESPONSE_LIMIT")
            return
        }
        inboundWrites.append(
            ProxyTCPInboundWrite(
                data: data,
                acknowledgedBytes: acknowledgedBytes
            )
        )
        writeNextInbound()
    }

    private func writeNextInbound() {
        guard !isWritingInbound,
              let next = inboundWrites.first,
              !isFinished
        else {
            return
        }
        isWritingInbound = true
        flow.write(next.data) { [weak self] error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                let written = self.inboundWrites.isEmpty
                    ? nil
                    : self.inboundWrites.removeFirst()
                self.isWritingInbound = false
                if let written {
                    self.budget.release(bytes: written.data.count)
                    if error == nil {
                        self.channel?.acknowledgeReceived(
                            bytes: written.acknowledgedBytes
                        )
                    }
                }
                if error != nil {
                    self.finish(code: "NP_PROXY_TCP_APP_WRITE_FAILED")
                } else {
                    self.writeNextInbound()
                    self.finishRemoteReadIfPossible()
                    self.finishAfterInboundWrites()
                }
            }
        }
    }

    private func finishRemoteReadIfPossible() {
        guard remoteReadFinished,
              !isWritingInbound,
              inboundWrites.isEmpty,
              !isFinished
        else {
            return
        }
        flow.closeWriteWithError(nil)
        finishIfComplete()
    }

    private func finishIfComplete() {
        guard appReadFinished, remoteReadFinished, !isFinished else {
            return
        }
        guard let channel else {
            finish(code: "NP_PROXY_CHANNEL_UNAVAILABLE")
            return
        }
        switch channel.send(
            type: .close,
            requiresCredit: false,
            completion: { [weak self] _ in
                self?.finish(error: nil)
            }
        ) {
        case .accepted:
            break
        case .insufficientCredit, .unavailable:
            finish(code: "NP_PROXY_CLOSE_FAILED")
        }
    }

    private func finishAfterInboundWrites() {
        guard gatewayClosed,
              !isWritingInbound,
              inboundWrites.isEmpty
        else {
            return
        }
        finish(error: nil)
    }

    private func finish(code: String) {
        finish(error: FlowRelayError.abort(code))
    }

    private func finish(error: Error?) {
        guard !isFinished else {
            return
        }
        isFinished = true
        finishError = error
        if let pendingAppData {
            budget.release(bytes: pendingAppData.count)
            self.pendingAppData = nil
        }
        budget.release(
            bytes: inboundWrites.reduce(0) { $0 + $1.data.count }
        )
        inboundWrites.removeAll(keepingCapacity: false)
        isWritingInbound = false
        channel?.cancel()
        channel = nil
        if isOpened {
            closeFlow(error: error)
            notifyFinish()
        } else if !isOpening {
            openFlow()
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
