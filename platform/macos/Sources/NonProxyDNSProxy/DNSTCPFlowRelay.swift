import Foundation
import NetworkExtension
import NonProxyProviderContracts
import NonProxyProviderCore

final class DNSTCPFlowRelay: DNSFlowRelay, @unchecked Sendable {
    let id = UUID()

    private let flow: NEAppProxyTCPFlow
    private let app: PolicyAppIdentity
    private let coordinator: DNSQueryCoordinator
    private let queue: DispatchQueue
    private let onFinish: @Sendable (UUID) -> Void
    private var framer = DNSTCPMessageFramer()
    private var isFinished = false
    private var workTask: Task<Void, Never>?

    init(
        flow: NEAppProxyTCPFlow,
        app: PolicyAppIdentity,
        coordinator: DNSQueryCoordinator,
        onFinish: @escaping @Sendable (UUID) -> Void
    ) {
        self.flow = flow
        self.app = app
        self.coordinator = coordinator
        self.onFinish = onFinish
        self.queue = DispatchQueue(label: "com.nonproxy.dns-tcp.\(id)")
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
                    self.readNextChunk()
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

    private func readNextChunk() {
        guard !isFinished else {
            return
        }
        flow.readData { [weak self] data, error in
            guard let self else {
                return
            }
            self.queue.async { [weak self] in
                guard let self, !self.isFinished else {
                    return
                }
                guard error == nil, let data, !data.isEmpty else {
                    self.finish(error: error)
                    return
                }
                do {
                    let messages = try self.framer.append(data)
                    if messages.isEmpty {
                        self.readNextChunk()
                    } else {
                        self.process(messages)
                    }
                } catch {
                    self.finish(error: error)
                }
            }
        }
    }

    private func process(_ messages: [Data]) {
        workTask = Task { [weak self] in
            guard let self else {
                return
            }
            var output = Data()
            do {
                for message in messages {
                    try Task.checkCancellation()
                    let response = await self.response(for: message)
                    if !response.isEmpty {
                        output.append(try DNSTCPMessageFramer.frame(response))
                    }
                }
            } catch {
                self.queue.async { [weak self] in
                    self?.finish(error: error)
                }
                return
            }
            guard !output.isEmpty else {
                self.queue.async { [weak self] in
                    self?.readNextChunk()
                }
                return
            }
            self.flow.write(output) { [weak self] error in
                guard let self else {
                    return
                }
                self.queue.async { [weak self] in
                    guard let self, !self.isFinished else {
                        return
                    }
                    if let error {
                        self.finish(error: error)
                    } else {
                        self.readNextChunk()
                    }
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
                    transport: .tcp
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
