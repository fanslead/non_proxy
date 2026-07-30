import Network
import NetworkExtension

// Network Extension flow 的所有可变状态都被私有串行队列隔离。
final class DirectUDPFlowRelay: FlowRelay, @unchecked Sendable {
    private static let maximumAssociations = 64
    private static let maximumQueuedBytes = 256 * 1024
    private static let openTimeout: DispatchTimeInterval = .seconds(10)

    private let flow: NEAppProxyUDPFlow
    private let interface: NWInterface
    private let budget: FlowRelayRegistry
    private let queue: DispatchQueue
    private let onFinish: @Sendable (DirectUDPFlowRelay) -> Void
    private var associations: [NWEndpoint: DirectUDPAssociation] = [:]
    private var responses: [(Data, NWEndpoint)] = []
    private var queuedBytes = 0
    private var responseBytes = 0
    private var isWritingResponse = false
    private var isOpening = false
    private var isOpened = false
    private var isFinished = false
    private var finishCode: String?
    private var didNotifyFinish = false

    init(
        flow: NEAppProxyUDPFlow,
        interface: NWInterface,
        budget: FlowRelayRegistry,
        queue: DispatchQueue,
        onFinish: @escaping @Sendable (DirectUDPFlowRelay) -> Void
    ) {
        self.flow = flow
        self.interface = interface
        self.budget = budget
        self.queue = queue
        self.onFinish = onFinish
    }

    func start() {
        queue.async { [weak self] in
            guard let self else {
                return
            }
            self.isOpening = true
            self.flow.open(withLocalFlowEndpoint: nil) { [weak self] error in
                guard let self else {
                    return
                }
                self.queue.async { [self] in
                    self.isOpening = false
                    if error != nil {
                        self.isFinished = true
                        self.notifyFinish()
                    } else if self.isFinished {
                        self.isOpened = true
                        self.closeFlow(
                            code: self.finishCode
                                ?? "NP_DIRECT_RELAY_CANCELLED"
                        )
                        self.notifyFinish()
                    } else {
                        self.isOpened = true
                        self.readNextBatch()
                    }
                }
            }
            self.queue.asyncAfter(
                deadline: .now() + Self.openTimeout
            ) { [weak self] in
                guard let self, self.isOpening, !self.isFinished else {
                    return
                }
                self.finish(code: "NP_DIRECT_UDP_OPEN_TIMEOUT")
            }
        }
    }

    func cancel() {
        queue.async { [weak self] in
            self?.finish(code: "NP_DIRECT_RELAY_CANCELLED")
        }
    }

    private func readNextBatch() {
        guard !isFinished else {
            return
        }
        flow.readDatagrams { [weak self] datagrams, error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                guard !self.isFinished else {
                    return
                }
                guard error == nil, let datagrams else {
                    self.finish(code: "NP_DIRECT_UDP_APP_READ_FAILED")
                    return
                }
                guard !datagrams.isEmpty else {
                    self.finish(code: "NP_DIRECT_UDP_APP_CLOSED")
                    return
                }
                for (data, endpoint) in datagrams {
                    guard self.enqueue(data, endpoint: endpoint) else {
                        self.finish(code: "NP_DIRECT_UDP_QUEUE_LIMIT")
                        return
                    }
                }
                self.readNextBatch()
            }
        }
    }

    private func enqueue(_ data: Data, endpoint: NWEndpoint) -> Bool {
        guard queuedBytes + data.count <= Self.maximumQueuedBytes,
              budget.reserve(bytes: data.count)
        else {
            return false
        }
        let association: DirectUDPAssociation
        if let existing = associations[endpoint] {
            association = existing
        } else {
            guard associations.count < Self.maximumAssociations else {
                budget.release(bytes: data.count)
                return false
            }
            association = makeAssociation(endpoint: endpoint)
            associations[endpoint] = association
            association.start()
        }
        queuedBytes += data.count
        let byteCount = data.count
        association.send(data) { [weak self] in
            guard let self else {
                return
            }
            self.queuedBytes = max(0, self.queuedBytes - byteCount)
            self.budget.release(bytes: byteCount)
        }
        return true
    }

    private func makeAssociation(
        endpoint: NWEndpoint
    ) -> DirectUDPAssociation {
        DirectUDPAssociation(
            endpoint: endpoint,
            interface: interface,
            queue: queue,
            onReceive: { [weak self] data, endpoint in
                self?.enqueueResponse(data, endpoint: endpoint)
            },
            onFailure: { [weak self] in
                self?.finish(code: "NP_DIRECT_UDP_REMOTE_FAILED")
            }
        )
    }

    private func enqueueResponse(_ data: Data, endpoint: NWEndpoint) {
        guard !isFinished,
              responseBytes + data.count <= Self.maximumQueuedBytes,
              budget.reserve(bytes: data.count)
        else {
            finish(code: "NP_DIRECT_UDP_RESPONSE_LIMIT")
            return
        }
        responses.append((data, endpoint))
        responseBytes += data.count
        writeNextResponse()
    }

    private func writeNextResponse() {
        guard !isFinished, !isWritingResponse, let next = responses.first else {
            return
        }
        isWritingResponse = true
        flow.writeDatagrams([next]) { [weak self] error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                guard !self.responses.isEmpty else {
                    return
                }
                let written = self.responses.removeFirst()
                self.responseBytes = max(
                    0,
                    self.responseBytes - written.0.count
                )
                self.budget.release(bytes: written.0.count)
                self.isWritingResponse = false
                if error != nil {
                    self.finish(code: "NP_DIRECT_UDP_APP_WRITE_FAILED")
                } else {
                    self.writeNextResponse()
                }
            }
        }
    }

    private func finish(code: String) {
        guard !isFinished else {
            return
        }
        isFinished = true
        finishCode = code
        associations.values.forEach { $0.cancel() }
        associations.removeAll(keepingCapacity: false)
        budget.release(bytes: responseBytes)
        responseBytes = 0
        responses.removeAll(keepingCapacity: false)
        if isOpened {
            closeFlow(code: code)
            notifyFinish()
        } else if !isOpening {
            notifyFinish()
        }
    }

    private func closeFlow(code: String) {
        let error = FlowRelayError.make(.aborted, nonProxyCode: code)
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
