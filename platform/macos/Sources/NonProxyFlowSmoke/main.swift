import Foundation
import Network
import NonProxyProviderCore

private let expectedPayload = Data("hello".utf8)

private final class FlowSmokeRunner: @unchecked Sendable {
    private let queue = DispatchQueue(label: "com.nonproxy.smoke.flow")
    private let connection: NWConnection
    private let capability: Data
    private let flowID: NPF1FlowID
    private let completion = DispatchSemaphore(value: 0)
    private var decoder = NPF1FrameDecoder()
    private var inboundSequence = NPF1SequenceTracker()
    private var outboundSequence: UInt64 = 0
    private var sentPayload = false
    private var receivedPayload = Data()
    private var completed = false
    private var failure: String?

    init(socketPath: String, capability: Data) throws {
        guard socketPath.hasPrefix("/"), capability.count == 32 else {
            throw NPF1ProtocolError.invalidPayload
        }
        connection = NWConnection(
            to: .unix(path: socketPath),
            using: .tcp
        )
        self.capability = capability
        flowID = try NPF1FlowID.random()
    }

    func run() -> String? {
        queue.async { [self] in
            connection.stateUpdateHandler = { [weak self] state in
                self?.handleState(state)
            }
            connection.start(queue: queue)
            queue.asyncAfter(deadline: .now() + .seconds(15)) { [weak self] in
                self?.fail("NP_FLOW_SMOKE_TIMEOUT")
            }
        }
        completion.wait()
        return failure
    }

    private func handleState(_ state: NWConnection.State) {
        guard !completed else {
            return
        }
        switch state {
        case .ready:
            sendOpen()
        case .failed:
            fail("NP_FLOW_SMOKE_CONNECTION_FAILED")
        case .cancelled:
            fail("NP_FLOW_SMOKE_CONNECTION_CANCELLED")
        case .setup, .preparing, .waiting:
            break
        @unknown default:
            fail("NP_FLOW_SMOKE_CONNECTION_UNKNOWN")
        }
    }

    private func sendOpen() {
        do {
            let payload = try NPF1PayloadCodec.encodeOpen(
                capability: capability,
                outboundID: "smoke-http",
                endpoint: NPF1Endpoint(host: "example.test", port: 443),
                initialWindowBytes: 256 * 1024
            )
            try send(type: .openTCP, payload: payload) { [weak self] sent in
                guard let self else {
                    return
                }
                if sent {
                    self.receive()
                }
            }
        } catch {
            fail("NP_FLOW_SMOKE_OPEN_INVALID")
        }
    }

    private func receive() {
        connection.receive(
            minimumIncompleteLength: 1,
            maximumLength: NPF1FrameCodec.headerBytes
                + NPF1FrameCodec.maximumPayloadBytes
        ) { [weak self] data, _, complete, error in
            guard let self, !self.completed else {
                return
            }
            if error != nil {
                self.fail("NP_FLOW_SMOKE_RECEIVE_FAILED")
                return
            }
            do {
                if let data, !data.isEmpty {
                    for frame in try self.decoder.append(data) {
                        try self.handle(frame)
                    }
                }
                if complete, !self.completed {
                    self.fail("NP_FLOW_SMOKE_RECEIVE_CLOSED")
                } else if !self.completed {
                    self.receive()
                }
            } catch {
                self.fail("NP_FLOW_SMOKE_PROTOCOL_INVALID")
            }
        }
    }

    private func handle(_ frame: NPF1Frame) throws {
        guard frame.flowID == flowID else {
            throw NPF1ProtocolError.invalidFlowID
        }
        try inboundSequence.accept(frame.sequence)
        switch frame.type {
        case .windowUpdate:
            _ = try NPF1PayloadCodec.decodeWindowUpdate(frame.payload)
            if !sentPayload {
                sentPayload = true
                try send(type: .data, payload: expectedPayload)
            }
        case .data:
            receivedPayload.append(frame.payload)
            guard receivedPayload.count <= expectedPayload.count,
                  expectedPayload.starts(with: receivedPayload)
            else {
                throw NPF1ProtocolError.invalidPayload
            }
            let receivedAll = receivedPayload == expectedPayload
            let update = try NPF1PayloadCodec.encodeWindowUpdate(
                UInt32(frame.payload.count)
            )
            try send(type: .windowUpdate, payload: update) { [weak self] sent in
                guard let self else {
                    return
                }
                if sent, receivedAll {
                    do {
                        try self.send(type: .close) { [weak self] closed in
                            if closed {
                                self?.succeed()
                            }
                        }
                    } catch {
                        self.fail("NP_FLOW_SMOKE_CLOSE_FAILED")
                    }
                }
            }
        case .error:
            fail(String(data: frame.payload, encoding: .utf8)
                ?? "NP_FLOW_SMOKE_GATEWAY_ERROR")
        case .halfClose, .close:
            fail("NP_FLOW_SMOKE_CLOSED_EARLY")
        case .openTCP, .openUDP, .datagram, .ping, .pong, .ready:
            throw NPF1ProtocolError.invalidFrameType
        }
    }

    private func send(
        type: NPF1FrameType,
        payload: Data = Data(),
        completion: (@Sendable (Bool) -> Void)? = nil
    ) throws {
        let frame = try NPF1Frame(
            type: type,
            flowID: flowID,
            sequence: outboundSequence,
            payload: payload
        )
        guard outboundSequence < UInt64.max else {
            throw NPF1ProtocolError.sequenceExhausted
        }
        outboundSequence += 1
        connection.send(
            content: try NPF1FrameCodec.encode(frame),
            completion: .contentProcessed { [weak self] error in
                guard let self, !self.completed else {
                    return
                }
                if error != nil {
                    self.fail("NP_FLOW_SMOKE_SEND_FAILED")
                } else {
                    completion?(true)
                }
            }
        )
    }

    private func succeed() {
        finish(failure: nil)
    }

    private func fail(_ code: String) {
        finish(failure: code)
    }

    private func finish(failure: String?) {
        guard !completed else {
            return
        }
        completed = true
        self.failure = failure
        connection.stateUpdateHandler = nil
        connection.cancel()
        completion.signal()
    }
}

private func main() -> Int32 {
    let arguments = CommandLine.arguments
    guard arguments.count == 3 else {
        FileHandle.standardError.write(
            Data("用法：NonProxyFlowSmoke <flow-socket> <capability-file>\n".utf8)
        )
        return 2
    }
    do {
        let capability = try Data(contentsOf: URL(fileURLWithPath: arguments[2]))
        let runner = try FlowSmokeRunner(
            socketPath: arguments[1],
            capability: capability
        )
        if let failure = runner.run() {
            FileHandle.standardError.write(
                Data("代理数据面跨语言联调失败：\(failure)\n".utf8)
            )
            return 1
        }
        print("代理数据面跨语言联调通过：NWConnection、NPF1、gatewayd 和 HTTP CONNECT 回显一致。")
        return 0
    } catch {
        FileHandle.standardError.write(
            Data("代理数据面跨语言联调初始化失败。\n".utf8)
        )
        return 1
    }
}

exit(main())
