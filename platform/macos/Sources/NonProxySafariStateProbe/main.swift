import Dispatch
import Foundation
import NonProxySafariStateBridge

@main
enum NonProxySafariStateProbe {
    static func main() {
        guard CommandLine.arguments.count == 2 else {
            writeStandardError(
                "用法：NonProxySafariStateProbe <扩展 Bundle ID>\n"
            )
            Foundation.exit(64)
        }
        let extensionIdentifier = CommandLine.arguments[1]
        guard !extensionIdentifier.isEmpty else {
            writeStandardError("Safari 扩展 Bundle ID 不能为空。\n")
            Foundation.exit(64)
        }

        let context = ProbeContext(
            extensionIdentifier: extensionIdentifier
        )
        let pointer = Unmanaged.passRetained(context).toOpaque()
        np_query_safari_extension_state(
            extensionIdentifier,
            safariStateCallback,
            pointer
        )
        DispatchQueue.global().asyncAfter(deadline: .now() + 15) {
            context.complete(
                available: false,
                enabled: false,
                errorMessage: "Safari 扩展状态查询超时。"
            )
        }
        dispatchMain()
    }
}

private final class ProbeContext: @unchecked Sendable {
    let extensionIdentifier: String
    private let lock = NSLock()
    private var completed = false

    init(extensionIdentifier: String) {
        self.extensionIdentifier = extensionIdentifier
    }

    func complete(
        available: Bool,
        enabled: Bool,
        errorMessage: String?
    ) {
        lock.lock()
        guard !completed else {
            lock.unlock()
            return
        }
        completed = true
        lock.unlock()

        var result: [String: Any] = [
            "schemaVersion": 1,
            "extensionIdentifier": extensionIdentifier,
            "available": available,
            "enabled": enabled,
        ]
        if let errorMessage {
            result["error"] = errorMessage
        }
        do {
            let data = try JSONSerialization.data(
                withJSONObject: result,
                options: [.sortedKeys, .withoutEscapingSlashes]
            )
            try FileHandle.standardOutput.write(contentsOf: data)
            try FileHandle.standardOutput.write(
                contentsOf: Data("\n".utf8)
            )
            Foundation.exit(0)
        } catch {
            writeStandardError("无法编码 Safari 扩展状态。\n")
            Foundation.exit(70)
        }
    }
}

private func safariStateCallback(
    _ available: Bool,
    _ enabled: Bool,
    _ errorMessage: UnsafePointer<CChar>?,
    _ contextPointer: UnsafeMutableRawPointer?
) {
    guard let contextPointer else {
        writeStandardError("Safari 扩展状态回调缺少上下文。\n")
        Foundation.exit(70)
    }
    let context = Unmanaged<ProbeContext>
        .fromOpaque(contextPointer)
        .takeUnretainedValue()
    context.complete(
        available: available,
        enabled: enabled,
        errorMessage: errorMessage.map(String.init(cString:))
    )
}

private func writeStandardError(_ message: String) {
    try? FileHandle.standardError.write(
        contentsOf: Data(message.utf8)
    )
}
