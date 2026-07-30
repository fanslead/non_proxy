import Foundation
import Network
import NonProxyProviderCore
@testable import NonProxyTransparentProxy
import XCTest

final class ProxyFlowChannelTests: XCTestCase {
    func testHandshakeCreditDataAndAcknowledgement() throws {
        let fixture = try ChannelFixture()
        fixture.start()
        let open = try fixture.firstSentFrame()
        XCTAssertEqual(open.type, .openTCP)
        XCTAssertEqual(open.sequence, 0)

        try fixture.deliver(
            type: .windowUpdate,
            sequence: 0,
            payload: NPF1PayloadCodec.encodeWindowUpdate(65_536)
        )
        XCTAssertTrue(fixture.events.containsReady)

        let sendResult = fixture.queue.sync {
            fixture.channel.send(
                type: .data,
                payload: Data("hello".utf8),
                requiresCredit: true
            )
        }
        XCTAssertSendAccepted(sendResult)
        let data = try fixture.sentFrame(at: 1)
        XCTAssertEqual(data.type, .data)
        XCTAssertEqual(data.sequence, 1)
        XCTAssertEqual(data.payload, Data("hello".utf8))

        try fixture.deliver(
            type: .data,
            sequence: 1,
            payload: Data("world".utf8)
        )
        XCTAssertEqual(fixture.events.lastFrame?.payload, Data("world".utf8))
        fixture.queue.sync {
            fixture.channel.acknowledgeReceived(bytes: 5)
        }
        let update = try fixture.sentFrame(at: 2)
        XCTAssertEqual(update.type, .windowUpdate)
        XCTAssertEqual(
            try NPF1PayloadCodec.decodeWindowUpdate(update.payload),
            5
        )
        fixture.stop()
    }

    func testInsufficientCreditPausesWithoutConsumingSequence() throws {
        let fixture = try ChannelFixture()
        fixture.start()
        _ = try fixture.firstSentFrame()
        try fixture.deliver(
            type: .windowUpdate,
            sequence: 0,
            payload: NPF1PayloadCodec.encodeWindowUpdate(4)
        )

        let blocked = fixture.queue.sync {
            fixture.channel.send(
                type: .data,
                payload: Data(repeating: 1, count: 5),
                requiresCredit: true
            )
        }
        XCTAssertSendInsufficientCredit(blocked)
        XCTAssertEqual(fixture.connection.sent.count, 1)

        try fixture.deliver(
            type: .windowUpdate,
            sequence: 1,
            payload: NPF1PayloadCodec.encodeWindowUpdate(1)
        )
        let accepted = fixture.queue.sync {
            fixture.channel.send(
                type: .data,
                payload: Data(repeating: 1, count: 5),
                requiresCredit: true
            )
        }
        XCTAssertSendAccepted(accepted)
        XCTAssertEqual(try fixture.sentFrame(at: 1).sequence, 1)
        fixture.stop()
    }

    func testWrongFlowIDClosesChannel() throws {
        let fixture = try ChannelFixture()
        fixture.start()
        _ = try fixture.firstSentFrame()
        let wrongID = try NPF1FlowID(bytes: Data(repeating: 8, count: 16))
        let frame = try NPF1Frame(
            type: .windowUpdate,
            flowID: wrongID,
            sequence: 2,
            payload: NPF1PayloadCodec.encodeWindowUpdate(1)
        )

        fixture.connection.deliver(try NPF1FrameCodec.encode(frame))

        XCTAssertEqual(
            fixture.events.lastFailure,
            "NP_PROXY_CHANNEL_PROTOCOL_INVALID"
        )
        XCTAssertTrue(fixture.connection.isCancelled)
    }

    func testOutOfOrderSequenceClosesChannel() throws {
        let fixture = try ChannelFixture()
        fixture.start()
        _ = try fixture.firstSentFrame()

        try fixture.deliver(
            type: .windowUpdate,
            sequence: 2,
            payload: NPF1PayloadCodec.encodeWindowUpdate(1)
        )

        XCTAssertEqual(
            fixture.events.lastFailure,
            "NP_PROXY_CHANNEL_PROTOCOL_INVALID"
        )
        XCTAssertTrue(fixture.connection.isCancelled)
    }

    func testReceiveWindowOverflowClosesChannelBeforeDeliveringFrame() throws {
        let fixture = try ChannelFixture()
        fixture.start()
        _ = try fixture.firstSentFrame()
        try fixture.deliver(
            type: .windowUpdate,
            sequence: 0,
            payload: NPF1PayloadCodec.encodeWindowUpdate(1)
        )
        try fixture.deliver(
            type: .data,
            sequence: 1,
            payload: Data(repeating: 1, count: 200 * 1024)
        )
        XCTAssertEqual(fixture.events.frameCount, 1)

        try fixture.deliver(
            type: .data,
            sequence: 2,
            payload: Data(repeating: 2, count: 100 * 1024)
        )

        XCTAssertEqual(fixture.events.frameCount, 1)
        XCTAssertEqual(
            fixture.events.lastFailure,
            "NP_PROXY_CHANNEL_PROTOCOL_INVALID"
        )
        XCTAssertTrue(fixture.connection.isCancelled)
    }
}

private final class ChannelFixture: @unchecked Sendable {
    let queue = DispatchQueue(label: "com.nonproxy.tests.proxy-channel")
    let connection = FakeProxyFlowStreamConnection()
    let events = ChannelEventRecorder()
    let channel: ProxyFlowChannel

    init() throws {
        let connection = connection
        let events = events
        channel = try ProxyFlowChannel(
            socketPath: "/tmp/nonproxy-flow.sock",
            capability: Data(repeating: 7, count: 32),
            outboundID: "proxy-main",
            endpoint: NPF1Endpoint(host: "example.test", port: 443),
            openType: .openTCP,
            queue: queue,
            connectionFactory: { _ in connection },
            onEvent: { event in
                events.append(event)
            }
        )
    }

    func start() {
        queue.sync {
            channel.start()
        }
    }

    func stop() {
        queue.sync {
            channel.cancel()
        }
    }

    func firstSentFrame() throws -> NPF1Frame {
        try sentFrame(at: 0)
    }

    func sentFrame(at index: Int) throws -> NPF1Frame {
        var decoder = NPF1FrameDecoder()
        return try XCTUnwrap(
            decoder.append(connection.sent[index]).first
        )
    }

    func deliver(
        type: NPF1FrameType,
        sequence: UInt64,
        payload: Data
    ) throws {
        let open = try firstSentFrame()
        let frame = try NPF1Frame(
            type: type,
            flowID: open.flowID,
            sequence: sequence,
            payload: payload
        )
        connection.deliver(try NPF1FrameCodec.encode(frame))
    }
}

private final class FakeProxyFlowStreamConnection:
    ProxyFlowStreamConnection,
    @unchecked Sendable
{
    var stateUpdateHandler: (@Sendable (NWConnection.State) -> Void)?
    private(set) var sent: [Data] = []
    private(set) var isCancelled = false
    private var receiver: (@Sendable (
        Data?,
        NWConnection.ContentContext?,
        Bool,
        NWError?
    ) -> Void)?

    func flowStart(queue: DispatchQueue) {
        stateUpdateHandler?(.ready)
    }

    func flowSend(
        content: Data?,
        completion: @escaping @Sendable (NWError?) -> Void
    ) {
        if let content {
            sent.append(content)
        }
        completion(nil)
    }

    func flowReceive(
        minimumIncompleteLength: Int,
        maximumLength: Int,
        completion: @escaping @Sendable (
            Data?,
            NWConnection.ContentContext?,
            Bool,
            NWError?
        ) -> Void
    ) {
        receiver = completion
    }

    func flowCancel() {
        isCancelled = true
        receiver = nil
    }

    func deliver(_ data: Data) {
        let callback = receiver
        receiver = nil
        callback?(data, nil, false, nil)
    }
}

private final class ChannelEventRecorder: @unchecked Sendable {
    private(set) var events: [ProxyFlowChannelEvent] = []

    var containsReady: Bool {
        events.contains {
            if case .ready = $0 {
                return true
            }
            return false
        }
    }

    var lastFrame: NPF1Frame? {
        events.reversed().compactMap {
            if case .frame(let frame) = $0 {
                return frame
            }
            return nil
        }.first
    }

    var lastFailure: String? {
        events.reversed().compactMap {
            if case .failed(let code) = $0 {
                return code
            }
            return nil
        }.first
    }

    var frameCount: Int {
        events.reduce(into: 0) { count, event in
            if case .frame = event {
                count += 1
            }
        }
    }

    func append(_ event: ProxyFlowChannelEvent) {
        events.append(event)
    }
}

private func XCTAssertSendAccepted(
    _ result: ProxyFlowChannelSendResult,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    guard case .accepted = result else {
        XCTFail("期望数据面写入被接受", file: file, line: line)
        return
    }
}

private func XCTAssertSendInsufficientCredit(
    _ result: ProxyFlowChannelSendResult,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    guard case .insufficientCredit = result else {
        XCTFail("期望数据面写入因窗口不足暂停", file: file, line: line)
        return
    }
}
