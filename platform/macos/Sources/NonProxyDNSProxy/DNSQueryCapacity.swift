public actor DNSQueryCapacity {
    private let limit: Int
    private var active = 0

    public init(limit: Int = 256) {
        self.limit = max(1, limit)
    }

    public func acquire() -> Bool {
        guard active < limit else {
            return false
        }
        active += 1
        return true
    }

    public func release() {
        active = max(0, active - 1)
    }

    public var activeCount: Int {
        active
    }
}
