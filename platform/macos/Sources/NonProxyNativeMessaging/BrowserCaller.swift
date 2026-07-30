import Foundation
import NonProxyMacRuntime

public struct BrowserCaller: Equatable, Sendable {
    public static let chromiumExtensionID =
        MacSharedRuntimePaths.chromiumExtensionID

    public let origin: String

    public init(arguments: [String]) throws {
        guard arguments.count >= 2 else {
            throw NativeMessagingError.invalidCaller
        }
        let value = arguments[1]
        let allowed = "chrome-extension://\(Self.chromiumExtensionID)/"
        guard value == allowed else {
            throw NativeMessagingError.invalidCaller
        }
        self.origin = value
    }

    init(validatedOrigin: String) {
        self.origin = validatedOrigin
    }
}
