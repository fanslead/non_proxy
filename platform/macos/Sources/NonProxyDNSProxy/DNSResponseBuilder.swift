import Foundation

public enum DNSResponseBuilder {
    public static func formatError(query: Data) -> Data {
        guard query.count >= 12 else {
            return Data()
        }
        var response = Data(query.prefix(12))
        let queryFlags = UInt16(response[2]) << 8 | UInt16(response[3])
        writeUInt16(
            queryFlags & 0x7910 | 0x8081,
            to: &response,
            at: 2
        )
        writeUInt16(0, to: &response, at: 4)
        writeUInt16(0, to: &response, at: 6)
        writeUInt16(0, to: &response, at: 8)
        writeUInt16(0, to: &response, at: 10)
        return response
    }

    public static func refused(
        query: Data,
        question: DNSQuestion
    ) -> Data {
        errorResponse(query: query, question: question, responseCode: 5)
    }

    public static func serverFailure(
        query: Data,
        question: DNSQuestion
    ) -> Data {
        errorResponse(query: query, question: question, responseCode: 2)
    }

    private static func errorResponse(
        query: Data,
        question: DNSQuestion,
        responseCode: UInt16
    ) -> Data {
        var response = Data(query.prefix(question.questionEndOffset))
        guard response.count >= 12 else {
            return Data()
        }
        let preservedFlags = question.flags & 0x7910
        writeUInt16(
            preservedFlags | 0x8080 | responseCode,
            to: &response,
            at: 2
        )
        writeUInt16(1, to: &response, at: 4)
        writeUInt16(0, to: &response, at: 6)
        writeUInt16(0, to: &response, at: 8)
        writeUInt16(0, to: &response, at: 10)
        return response
    }

    private static func writeUInt16(
        _ value: UInt16,
        to data: inout Data,
        at offset: Int
    ) {
        data[offset] = UInt8((value >> 8) & 0xFF)
        data[offset + 1] = UInt8(value & 0xFF)
    }
}
