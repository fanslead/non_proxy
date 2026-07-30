import Foundation

struct CanonicalBytes {
    private(set) var data = Data()

    mutating func appendByte(_ byte: UInt8) {
        data.append(byte)
    }

    mutating func appendData(_ bytes: Data) {
        data.append(bytes)
    }

    mutating func appendUInt16(_ value: UInt16) {
        appendByte(UInt8((value >> 8) & 0xff))
        appendByte(UInt8(value & 0xff))
    }

    mutating func appendUInt32(_ value: UInt32) {
        appendByte(UInt8((value >> 24) & 0xff))
        appendByte(UInt8((value >> 16) & 0xff))
        appendByte(UInt8((value >> 8) & 0xff))
        appendByte(UInt8(value & 0xff))
    }

    mutating func appendInt32(_ value: Int32) {
        appendUInt32(UInt32(bitPattern: value))
    }

    mutating func appendUInt64(_ value: UInt64) {
        appendByte(UInt8((value >> 56) & 0xff))
        appendByte(UInt8((value >> 48) & 0xff))
        appendByte(UInt8((value >> 40) & 0xff))
        appendByte(UInt8((value >> 32) & 0xff))
        appendByte(UInt8((value >> 24) & 0xff))
        appendByte(UInt8((value >> 16) & 0xff))
        appendByte(UInt8((value >> 8) & 0xff))
        appendByte(UInt8(value & 0xff))
    }

    mutating func appendBytes(_ bytes: Data) {
        appendUInt64(UInt64(bytes.count))
        appendData(bytes)
    }

    mutating func appendString(_ value: String) {
        appendBytes(Data(value.utf8))
    }

    mutating func appendOptionalString(_ value: String?) {
        guard let value else {
            appendByte(0)
            return
        }
        appendByte(1)
        appendString(value)
    }
}
