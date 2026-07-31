import Foundation
import Network

// NWConnection 回调与队列操作都在所属 relay 的私有串行队列执行。
final class DirectUDPAssociation: @unchecked Sendable {
    private static let establishmentTimeout: DispatchTimeInterval = .seconds(15)

    private struct PendingDatagram {
        let data: Data
        let didSend: @Sendable () -> Void
    }

    private let endpoint: NWEndpoint
    private let connection: NWConnection
    private let queue: DispatchQueue
    private let onReady: @Sendable () -> Void
    private let onReceive: @Sendable (Data, NWEndpoint) -> Void
    private let onFailure: @Sendable () -> Void
    private var pending: [PendingDatagram] = []
    private var isReady = false
    private var isSending = false
    private var isCancelled = false

    init(
        endpoint: NWEndpoint,
        interface: NWInterface,
        queue: DispatchQueue,
        onReady: @escaping @Sendable () -> Void,
        onReceive: @escaping @Sendable (Data, NWEndpoint) -> Void,
        onFailure: @escaping @Sendable () -> Void
    ) {
        self.endpoint = endpoint
        connection = DirectConnectionFactory.makeUDP(
            endpoint: endpoint,
            interface: interface
        )
        self.queue = queue
        self.onReady = onReady
        self.onReceive = onReceive
        self.onFailure = onFailure
    }

    func start() {
        connection.stateUpdateHandler = { [weak self] state in
            self?.handleState(state)
        }
        connection.start(queue: queue)
        queue.asyncAfter(
            deadline: .now() + Self.establishmentTimeout
        ) { [weak self] in
            guard let self, !self.isReady, !self.isCancelled else {
                return
            }
            self.fail()
        }
    }

    func send(_ data: Data, didSend: @escaping @Sendable () -> Void) {
        guard !isCancelled else {
            didSend()
            return
        }
        pending.append(PendingDatagram(data: data, didSend: didSend))
        sendNextIfPossible()
    }

    func cancel() {
        guard !isCancelled else {
            return
        }
        close()
    }

    private func handleState(_ state: NWConnection.State) {
        guard !isCancelled else {
            return
        }
        switch state {
        case .ready:
            isReady = true
            onReady()
            sendNextIfPossible()
            receiveNext()
        case .failed, .cancelled:
            fail()
        default:
            break
        }
    }

    private func sendNextIfPossible() {
        guard isReady, !isSending, let next = pending.first else {
            return
        }
        isSending = true
        connection.send(
            content: next.data,
            contentContext: .defaultMessage,
            isComplete: true,
            completion: .contentProcessed { [weak self] error in
                guard let self, !self.pending.isEmpty else {
                    return
                }
                let sent = self.pending.removeFirst()
                self.isSending = false
                sent.didSend()
                if error != nil {
                    self.onFailure()
                } else {
                    self.sendNextIfPossible()
                }
            }
        )
    }

    private func receiveNext() {
        guard isReady, !isCancelled else {
            return
        }
        connection.receiveMessage { [weak self] data, _, _, error in
            guard let self, !self.isCancelled else {
                return
            }
            if error != nil {
                self.fail()
                return
            }
            if let data, !data.isEmpty {
                self.onReceive(data, self.endpoint)
            }
            self.receiveNext()
        }
    }

    private func fail() {
        guard !isCancelled else {
            return
        }
        close()
        onFailure()
    }

    private func close() {
        isCancelled = true
        connection.stateUpdateHandler = nil
        connection.cancel()
        let callbacks = pending.map(\.didSend)
        pending.removeAll(keepingCapacity: false)
        callbacks.forEach { $0() }
    }
}
