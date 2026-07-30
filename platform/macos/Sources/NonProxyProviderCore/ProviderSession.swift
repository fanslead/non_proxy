import Foundation
import NonProxyProviderContracts

public actor ProviderSession {
    public let instanceID: String
    private var token = Data()
    private var nextSequence: UInt64 = 1
    private var expiresAt = Date.distantPast
    private var generation: UInt64 = 0

    public init(instanceID: String = UUID().uuidString.lowercased()) {
        self.instanceID = instanceID
    }

    public func install(
        response: Nonproxy_Provider_V1_RegisterProviderResponse
    ) throws {
        guard response.accepted else {
            let message = response.hasError ? response.error.message : "gatewayd 拒绝 Provider 注册"
            throw ProviderError.registrationRejected(message)
        }
        guard response.sessionToken.count == 32,
              response.providerGeneration > 0,
              response.hasSessionExpiresAt
        else {
            throw ProviderError.invalidSession("gatewayd 返回的 Provider 会话不完整")
        }

        let timestamp = response.sessionExpiresAt
        guard timestamp.nanos >= 0, timestamp.nanos < 1_000_000_000 else {
            throw ProviderError.invalidSession("Provider 会话过期时间无效")
        }
        let seconds = TimeInterval(timestamp.seconds)
        let nanoseconds = TimeInterval(timestamp.nanos) / 1_000_000_000
        token = response.sessionToken
        expiresAt = Date(timeIntervalSince1970: seconds + nanoseconds)
        generation = response.providerGeneration
        nextSequence = 1
    }

    public func requestContext(
        now: Date = Date()
    ) throws -> Nonproxy_Provider_V1_ProviderRequestContext {
        guard token.count == 32, generation > 0, now < expiresAt else {
            throw ProviderError.invalidSession("Provider 会话未建立或已经过期")
        }
        guard nextSequence < UInt64.max else {
            throw ProviderError.invalidSession("Provider 请求序号已经耗尽")
        }

        var context = Nonproxy_Provider_V1_ProviderRequestContext()
        context.providerInstanceID = instanceID
        context.sessionToken = token
        context.requestSequence = nextSequence
        nextSequence += 1
        return context
    }

    public func remainingLifetime(now: Date = Date()) -> TimeInterval {
        max(0, expiresAt.timeIntervalSince(now))
    }
}
