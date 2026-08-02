use nonproxy_flow_protocol::{FlowReady, FrameType, WindowUpdate};
use nonproxy_model::OutboundId;

use super::{FlowFrameSender, FlowServiceError};

pub(super) async fn send_initial_ready(
    sender: &FlowFrameSender,
    selected_outbound: Option<&OutboundId>,
    receive_window_bytes: u32,
) -> Result<(), FlowServiceError> {
    let (frame_type, payload) = match selected_outbound {
        Some(outbound_id) => (
            FrameType::Ready,
            FlowReady::new(outbound_id.clone(), receive_window_bytes)?.encode()?,
        ),
        None => (
            FrameType::WindowUpdate,
            WindowUpdate::new(receive_window_bytes)?.encode().to_vec(),
        ),
    };
    sender.send(frame_type, payload).await
}
