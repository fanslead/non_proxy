public struct ProviderHealthMetrics: Sendable, Equatable {
    public let activeFlowCount: UInt64
    public let queuedBytes: UInt64

    public init(activeFlowCount: UInt64 = 0, queuedBytes: UInt64 = 0) {
        self.activeFlowCount = activeFlowCount
        self.queuedBytes = queuedBytes
    }

    public static let idle = Self()
}
