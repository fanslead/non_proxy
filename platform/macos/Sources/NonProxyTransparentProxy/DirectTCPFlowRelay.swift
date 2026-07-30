import Network
import NetworkExtension

// Network Extension flow 的所有可变状态都被私有串行队列隔离。
final class DirectTCPFlowRelay: FlowRelay, @unchecked Sendable {
    private static let maximumReadBytes = 64 * 1024
    private static let establishmentTimeout: DispatchTimeInterval = .seconds(15)

    private let flow: NEAppProxyTCPFlow
    private let connection: NWConnection
    private let budget: FlowRelayRegistry
    private let queue: DispatchQueue
    private let onFinish: @Sendable (DirectTCPFlowRelay) -> Void
    private var connectionReady = false
    private var isOpening = false
    private var isOpened = false
    private var appReadFinished = false
    private var remoteReadFinished = false
    private var isFinished = false
    private var finishError: Error?
    private var didNotifyFinish = false
    private var appWriteBytes = 0
    private var remoteWriteBytes = 0

    init(
        flow: NEAppProxyTCPFlow,
        connection: NWConnection,
        budget: FlowRelayRegistry,
        queue: DispatchQueue,
        onFinish: @escaping @Sendable (DirectTCPFlowRelay) -> Void
    ) {
        self.flow = flow
        self.connection = connection
        self.budget = budget
        self.queue = queue
        self.onFinish = onFinish
    }

    func start() {
        queue.async { [weak self] in
            guard let self else {
                return
            }
            self.openFlow()
            self.connection.stateUpdateHandler = { [weak self] state in
                self?.handleConnectionState(state)
            }
            self.connection.start(queue: self.queue)
            self.queue.asyncAfter(
                deadline: .now() + Self.establishmentTimeout
            ) { [weak self] in
                guard let self,
                      !self.isFinished,
                      !(self.connectionReady && self.isOpened)
                else {
                    return
                }
                self.finish(
                    error: FlowRelayError.make(
                        .timedOut,
                        nonProxyCode: "NP_DIRECT_CONNECT_TIMEOUT"
                    )
                )
            }
        }
    }

    func cancel() {
        queue.async { [weak self] in
            self?.finish(
                error: FlowRelayError.make(
                    .aborted,
                    nonProxyCode: "NP_DIRECT_RELAY_CANCELLED"
                )
            )
        }
    }

    private func handleConnectionState(_ state: NWConnection.State) {
        guard !isFinished else {
            return
        }
        switch state {
        case .ready:
            connectionReady = true
            startPumpsIfReady()
        case .failed:
            finish(
                error: FlowRelayError.make(
                    .hostUnreachable,
                    nonProxyCode: "NP_DIRECT_CONNECT_FAILED"
                )
            )
        case .cancelled:
            finish(error: nil)
        default:
            break
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
                    self.connection.cancel()
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

    private func startPumpsIfReady() {
        guard connectionReady, isOpened, !isFinished else {
            return
        }
        readFromApp()
        readFromRemote()
    }

    private func readFromApp() {
        guard !isFinished, !appReadFinished else {
            return
        }
        flow.readData { [weak self] data, error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                handleAppData(data, error: error)
            }
        }
    }

    private func handleAppData(_ data: Data?, error: Error?) {
        guard !isFinished else {
            return
        }
        if error != nil {
            finish(
                error: FlowRelayError.make(
                    .peerReset,
                    nonProxyCode: "NP_DIRECT_APP_READ_FAILED"
                )
            )
            return
        }
        guard let data, !data.isEmpty else {
            appReadFinished = true
            connection.send(
                content: nil,
                contentContext: .finalMessage,
                isComplete: true,
                completion: .contentProcessed { [weak self] error in
                    guard let self else {
                        return
                    }
                    if error != nil {
                        self.finish(
                            error: FlowRelayError.make(
                                .peerReset,
                                nonProxyCode: "NP_DIRECT_HALF_CLOSE_FAILED"
                            )
                        )
                    } else {
                        self.finishIfComplete()
                    }
                }
            )
            return
        }
        guard budget.reserve(bytes: data.count) else {
            finish(
                error: FlowRelayError.make(
                    .aborted,
                    nonProxyCode: "NP_DIRECT_TCP_QUEUE_LIMIT"
                )
            )
            return
        }
        appWriteBytes = data.count
        let byteCount = data.count
        connection.send(
            content: data,
            completion: .contentProcessed { [weak self] error in
                guard let self else {
                    return
                }
                self.releaseAppWriteBudget(expected: byteCount)
                if error != nil {
                    self.finish(
                        error: FlowRelayError.make(
                            .peerReset,
                            nonProxyCode: "NP_DIRECT_REMOTE_WRITE_FAILED"
                        )
                    )
                } else {
                    self.readFromApp()
                }
            }
        )
    }

    private func readFromRemote() {
        guard !isFinished, !remoteReadFinished else {
            return
        }
        connection.receive(
            minimumIncompleteLength: 1,
            maximumLength: Self.maximumReadBytes
        ) { [weak self] data, _, isComplete, error in
            guard let self else {
                return
            }
            self.handleRemoteData(
                data,
                isComplete: isComplete,
                error: error
            )
        }
    }

    private func handleRemoteData(
        _ data: Data?,
        isComplete: Bool,
        error: Error?
    ) {
        guard !isFinished else {
            return
        }
        if error != nil {
            finish(
                error: FlowRelayError.make(
                    .peerReset,
                    nonProxyCode: "NP_DIRECT_REMOTE_READ_FAILED"
                )
            )
            return
        }
        let completeAfterWrite = isComplete
        guard let data, !data.isEmpty else {
            if isComplete {
                finishRemoteRead()
            } else {
                readFromRemote()
            }
            return
        }
        guard budget.reserve(bytes: data.count) else {
            finish(
                error: FlowRelayError.make(
                    .aborted,
                    nonProxyCode: "NP_DIRECT_TCP_QUEUE_LIMIT"
                )
            )
            return
        }
        remoteWriteBytes = data.count
        let byteCount = data.count
        flow.write(data) { [weak self] error in
            guard let self else {
                return
            }
            self.queue.async { [self] in
                self.releaseRemoteWriteBudget(expected: byteCount)
                if error != nil {
                    self.finish(
                        error: FlowRelayError.make(
                            .peerReset,
                            nonProxyCode: "NP_DIRECT_APP_WRITE_FAILED"
                        )
                    )
                } else if completeAfterWrite {
                    self.finishRemoteRead()
                } else {
                    self.readFromRemote()
                }
            }
        }
    }

    private func finishRemoteRead() {
        remoteReadFinished = true
        flow.closeWriteWithError(nil)
        finishIfComplete()
    }

    private func finishIfComplete() {
        if appReadFinished && remoteReadFinished {
            finish(error: nil)
        }
    }

    private func finish(error: Error?) {
        guard !isFinished else {
            return
        }
        isFinished = true
        finishError = error
        releaseAppWriteBudget(expected: appWriteBytes)
        releaseRemoteWriteBudget(expected: remoteWriteBytes)
        connection.stateUpdateHandler = nil
        connection.cancel()
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

    private func releaseAppWriteBudget(expected: Int) {
        let released = min(appWriteBytes, expected)
        appWriteBytes -= released
        budget.release(bytes: released)
    }

    private func releaseRemoteWriteBudget(expected: Int) {
        let released = min(remoteWriteBytes, expected)
        remoteWriteBytes -= released
        budget.release(bytes: released)
    }
}
