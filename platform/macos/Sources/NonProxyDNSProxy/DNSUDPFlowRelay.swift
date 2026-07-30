import Foundation
import Network
import NetworkExtension
import NonProxyProviderContracts
import NonProxyProviderCore

final class DNSUDPFlowRelay: DNSFlowRelay, @unchecked Sendable {
    let id = UUID()

    private let flow: NEAppProxyUDPFlow
    private let app: PolicyAppIdentity
    private let coordinator: DNSQueryCoordinator
    private let queue: DispatchQueue
    private let onFinish: @Sendable (UUID) -> Void
    private var isFinished = false
    private var workTask: Task<Void, Never>?

    init(
        flow: NEAppProxyUDPFlow,
        app: PolicyAppIdentity,
        coordinator: DNSQueryCoordinator,
        onFinish: @escaping @Sendable (UUID) -> Void
    ) {
        self.flow = flow
        self.app = app
        self.coordinator = coordinator
        self.onFinish = onFinish
        self.queue = DispatchQueue(label: "com.nonproxy.dns-udp.\(id)")
    }

    func start() {
        queue.async { [weak self] in
            guard let self, !self.isFinished else {
                return
            }
            self.flow.open(withLocalFlowEndpoint: nil) { [weak self] error in
                guard let self else {
                    return
                }
                self.queue.async { [weak self] in
                    guard let self, !self.isFinished else {
                        return
                    }
                    guard error == nil else {
                        self.finish(error: nil)
                        return
                    }
                    self.readNextBatch()
                }
            }
        }
    }

    func cancel() {
        queue.async { [weak self] in
            self?.finish(
                error: DNSFlowErrorFactory.make(
                    .aborted,
                    nonProxyCode: "NP_DNS_FLOW_CANCELLED"
                )
            )
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
            self.queue.async { [weak self] in
                guard let self, !self.isFinished else {
                    return
                }
                guard error == nil, let datagrams, !datagrams.isEmpty else {
                    self.finish(error: error)
                    return
                }
                self.process(datagrams)
            }
        }
    }

    private func process(_ datagrams: [(Data, NWEndpoint)]) {
        workTask = Task { [weak self] in
            guard let self else {
                return
            }
            var responses: [(Data, NWEndpoint)] = []
            for (message, endpoint) in datagrams {
                if Task.isCancelled {
                    return
                }
                let response = await self.response(for: message)
                if !response.isEmpty {
                    responses.append((response, endpoint))
                }
            }
            guard !responses.isEmpty else {
                self.queue.async { [weak self] in
                    self?.readNextBatch()
                }
                return
            }
            do {
                try await self.flow.writeDatagrams(responses)
                self.queue.async { [weak self] in
                    self?.readNextBatch()
                }
            } catch {
                self.queue.async { [weak self] in
                    self?.finish(error: error)
                }
            }
        }
    }

    private func response(for message: Data) async -> Data {
        do {
            return try await coordinator.resolve(
                DNSFlowQueryContext(
                    message: message,
                    app: app,
                    transport: .udp
                )
            )
        } catch {
            if let question = try? DNSMessageParser.parseQuery(message) {
                return DNSResponseBuilder.serverFailure(
                    query: message,
                    question: question
                )
            }
            return DNSResponseBuilder.formatError(query: message)
        }
    }

    private func finish(error: Error?) {
        guard !isFinished else {
            return
        }
        isFinished = true
        workTask?.cancel()
        workTask = nil
        flow.closeReadWithError(error)
        flow.closeWriteWithError(error)
        onFinish(id)
    }
}
