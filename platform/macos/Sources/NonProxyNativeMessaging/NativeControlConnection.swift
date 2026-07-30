import GRPCCore
import GRPCNIOTransportHTTP2Posix
import NonProxyProviderContracts
import Synchronization

public final class NativeControlConnection: Sendable {
    public typealias ServiceClient =
        Nonproxy_Control_V1_ControlService.Client<
            HTTP2ClientTransport.Posix
        >

    private enum Status: Sendable {
        case running
        case stopping
        case stopped
    }

    private let status = Mutex<Status>(.running)
    private let client: GRPCClient<HTTP2ClientTransport.Posix>
    private let serviceClient: ServiceClient

    public init(socketPath: String) throws {
        let transport = try HTTP2ClientTransport.Posix(
            target: .unixDomainSocket(
                path: socketPath,
                authority: "localhost"
            ),
            transportSecurity: .plaintext
        )
        let client = GRPCClient(transport: transport)
        self.client = client
        self.serviceClient = ServiceClient(wrapping: client)
        Task {
            do {
                try await client.runConnections()
            } catch {
                // RPC 调用会返回具体连接错误，后台连接任务不得写 stdout。
            }
            self.status.withLock {
                $0 = .stopped
            }
        }
    }

    public func perform<Result: Sendable>(
        _ operation: @Sendable (ServiceClient) async throws -> Result
    ) async throws -> Result {
        let available = status.withLock {
            if case .running = $0 {
                return true
            }
            return false
        }
        guard available else {
            throw NativeMessagingError.runtimeUnavailable(
                "NonProxy 本地控制连接不可用。"
            )
        }
        return try await operation(serviceClient)
    }

    public func shutdown() {
        let shouldStop = status.withLock {
            guard case .running = $0 else {
                return false
            }
            $0 = .stopping
            return true
        }
        if shouldStop {
            client.beginGracefulShutdown()
        }
    }
}
