import Foundation

public struct DNSQuestion: Equatable, Sendable {
    public let transactionID: UInt16
    public let flags: UInt16
    public let name: String
    public let type: UInt16
    public let queryClass: UInt16
    public let questionEndOffset: Int

    public init(
        transactionID: UInt16,
        flags: UInt16,
        name: String,
        type: UInt16,
        queryClass: UInt16,
        questionEndOffset: Int
    ) {
        self.transactionID = transactionID
        self.flags = flags
        self.name = name
        self.type = type
        self.queryClass = queryClass
        self.questionEndOffset = questionEndOffset
    }
}
