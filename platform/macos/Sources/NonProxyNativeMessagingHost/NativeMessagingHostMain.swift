import Foundation
import NonProxyNativeMessaging

@main
struct NonProxyNativeMessagingHost {
    static func main() async {
        do {
            _ = try BrowserCaller(arguments: CommandLine.arguments)
            let configuration = try NativeHostRuntimeConfiguration.live()
            let service = try NativeLearningClient(
                configuration: configuration
            )
            let runner = NativeHostRunner(
                handler: NativeRequestHandler(service: service)
            )
            try await runner.run()
        } catch {
            let message =
                "NonProxy Native Messaging Host 启动或通信失败。\n"
            try? FileHandle.standardError.write(
                contentsOf: Data(message.utf8)
            )
            Foundation.exit(1)
        }
    }
}
