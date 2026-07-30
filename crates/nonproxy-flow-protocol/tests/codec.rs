use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use nonproxy_flow_protocol::{
    FRAME_HEADER_BYTES, FlowEndpoint, FlowFrame, FlowId, FrameType, OpenFlowRequest,
    SequenceTracker, WindowUpdate, read_frame, write_frame,
};
use nonproxy_model::OutboundId;

#[tokio::test]
async fn frame_round_trip_preserves_header_and_sensitive_open_payload() {
    let flow_id = flow_id();
    let frame = FlowFrame::new(FrameType::OpenTcp, 0, flow_id, 0, vec![7, 8, 9]);
    let Ok(frame) = frame else {
        panic!("测试帧创建失败: {frame:?}");
    };
    let mut bytes = Vec::new();
    if let Err(error) = write_frame(&mut bytes, &frame).await {
        panic!("测试帧编码失败: {error}");
    }

    assert_eq!(bytes.len(), FRAME_HEADER_BYTES + 3);
    let mut expected = Vec::new();
    expected.extend_from_slice(b"NPF1");
    expected.extend_from_slice(&[0, 1, 1, 0]);
    expected.extend_from_slice(&[1; 16]);
    expected.extend_from_slice(&[0; 8]);
    expected.extend_from_slice(&[0, 0, 0, 3, 7, 8, 9]);
    assert_eq!(bytes, expected);
    let decoded = read_frame(&mut bytes.as_slice()).await;
    let Ok(decoded) = decoded else {
        panic!("测试帧解码失败: {decoded:?}");
    };
    assert_eq!(decoded.frame_type(), FrameType::OpenTcp);
    assert_eq!(decoded.flow_id(), flow_id);
    assert_eq!(decoded.sequence(), 0);
    assert_eq!(decoded.payload(), [7, 8, 9]);
    assert!(!format!("{decoded:?}").contains("7, 8, 9"));
}

#[test]
fn open_request_round_trip_normalizes_domain_and_redacts_capability() {
    let outbound = OutboundId::new("primary");
    let endpoint = FlowEndpoint::new("Proxy.Example.com.", 443);
    let (Ok(outbound), Ok(endpoint)) = (outbound, endpoint) else {
        panic!("测试 OPEN 参数创建失败");
    };
    let request = OpenFlowRequest::new([0xAB; 32], outbound, endpoint, 65_536);
    let Ok(request) = request else {
        panic!("测试 OPEN 请求创建失败: {request:?}");
    };
    let encoded = request.encode();
    let Ok(encoded) = encoded else {
        panic!("测试 OPEN 请求编码失败: {encoded:?}");
    };
    let decoded = OpenFlowRequest::decode(encoded.as_slice());
    let Ok(decoded) = decoded else {
        panic!("测试 OPEN 请求解码失败: {decoded:?}");
    };

    assert_eq!(decoded.outbound_id().as_str(), "primary");
    assert_eq!(decoded.endpoint().host(), "proxy.example.com");
    assert_eq!(decoded.endpoint().port(), 443);
    assert_eq!(decoded.initial_window_bytes(), 65_536);
    assert!(!format!("{decoded:?}").contains("abab"));
}

#[test]
fn ipv6_datagram_and_window_use_network_byte_order() {
    let endpoint = FlowEndpoint::Ip(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 53));
    let datagram = nonproxy_flow_protocol::DatagramPayload::new(endpoint.clone(), vec![0x12, 0x34]);
    let Ok(datagram) = datagram else {
        panic!("测试数据报创建失败: {datagram:?}");
    };
    let encoded = datagram.encode();
    let Ok(encoded) = encoded else {
        panic!("测试数据报编码失败: {encoded:?}");
    };
    let decoded = nonproxy_flow_protocol::DatagramPayload::decode(&encoded);
    assert!(matches!(
        decoded,
        Ok(value) if value.endpoint() == &endpoint && value.content() == [0x12, 0x34]
    ));

    let window = WindowUpdate::new(0x0102_0304);
    assert!(matches!(
        window,
        Ok(value) if value.encode() == [1, 2, 3, 4]
    ));
}

#[test]
fn sequence_tracker_rejects_replay_and_gaps() {
    let mut tracker = SequenceTracker::default();
    assert!(tracker.accept(0).is_ok());
    assert!(tracker.accept(0).is_err());
    assert!(tracker.accept(2).is_err());
    assert_eq!(tracker.expected(), 1);
    assert!(tracker.accept(1).is_ok());
}

#[tokio::test]
async fn decoder_rejects_unknown_version_oversized_length_and_truncation() {
    let mut unsupported = header(2, 3);
    unsupported.extend_from_slice(&[1, 2, 3]);
    assert!(read_frame(&mut unsupported.as_slice()).await.is_err());

    let oversized = header(1, 256 * 1024 + 1);
    assert!(read_frame(&mut oversized.as_slice()).await.is_err());

    let mut truncated = header(1, 3);
    truncated.extend_from_slice(&[1, 2]);
    assert!(read_frame(&mut truncated.as_slice()).await.is_err());
}

fn header(version: u16, payload_length: u32) -> Vec<u8> {
    let mut value = Vec::new();
    value.extend_from_slice(b"NPF1");
    value.extend_from_slice(&version.to_be_bytes());
    value.extend_from_slice(&[3, 0]);
    value.extend_from_slice(&[1; 16]);
    value.extend_from_slice(&[0; 8]);
    value.extend_from_slice(&payload_length.to_be_bytes());
    value
}

fn flow_id() -> FlowId {
    let result = FlowId::new([1; 16]);
    let Ok(value) = result else {
        panic!("测试 flow 标识创建失败: {result:?}");
    };
    value
}
