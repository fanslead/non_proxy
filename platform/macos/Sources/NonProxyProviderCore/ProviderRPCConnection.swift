import Foundation
import GRPCCore
import GRPCNIOTransportHTTP2Posix
import NonProxyProviderContracts
import Synchronization

public final class ProviderRPCConnection: Sendable {
    public typealias ServiceClient =
        Nonproxy_Provider_V1_ProviderService.Client<
            HTTP2ClientTransport.Posix
        >

    private enum Status: Sendable {
        case running
        case stopping
        case stopped(String?)
    }

    private final class ConnectionState: Sendable {
        private let status = Mutex<Status>(.running)

        func beginShutdown() -> Bool {
            status.withLock {
                guard case .running = $0 else {
                    return false
                }
                $0 = .stopping
                return true
            }
        }

        func recordStop(error: Error?) {
            status.withLock {
                $0 = .stopped(error.map { String(reflecting: $0) })
            }
        }

        func ensureAvailable() throws {
            try status.withLock {
                switch $0 {
                case .running:
                    return
                case .stopping:
                    throw ProviderError.control(
                        "Provider 控制连接正在停止"
                    )
                case .stopped(let detail):
                    let suffix = detail.map { "：\($0)" } ?? ""
                    throw ProviderError.control(
                        "Provider 控制连接已经停止\(suffix)"
                    )
                }
            }
        }
    }

    private let client: GRPCClient<HTTP2ClientTransport.Posix>
    private let serviceClient: ServiceClient
    private let connectionState: ConnectionState
    private let runTask: Task<Void, Never>

    public init(socketPath: String) throws {
        let transport = try HTTP2ClientTransport.Posix(
            target: .unixDomainSocket(
                path: socketPath,
                authority: "localhost"
            ),
            transportSecurity: .plaintext
        )
        let grpcClient = GRPCClient(transport: transport)
        let state = ConnectionState()
        self.client = grpcClient
        self.serviceClient = ServiceClient(wrapping: grpcClient)
        self.connectionState = state
        self.runTask = Task {
            do {
                try await grpcClient.runConnections()
                state.recordStop(error: nil)
            } catch {
                state.recordStop(error: error)
            }
        }
    }

    public func perform<Result: Sendable>(
        _ operation: @Sendable (ServiceClient) async throws -> Result
    ) async throws -> Result {
        try connectionState.ensureAvailable()
        return try await operation(serviceClient)
    }

    public func shutdown() {
        guard connectionState.beginShutdown() else {
            return
        }
        client.beginGracefulShutdown()
    }
}
