use std::{sync::Arc, time::Duration};

use nonproxy_flow_protocol::{
    FlowFrame, FlowId, FrameType, OpenFlowRequest, SequenceTracker, read_frame, write_frame,
};
use nonproxy_outbound::OutboundConnector;
use tokio::{net::UnixStream, time::timeout};

use crate::{Gateway, credential_store::CredentialStore, session_capability::SessionCapability};

use super::{FlowServiceError, load_connector, relay_tcp, relay_udp};

const OPEN_FRAME_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct FlowConnectionHandler {
    gateway: Gateway,
    session: SessionCapability,
    credential_store: Arc<dyn CredentialStore>,
}

impl FlowConnectionHandler {
    pub fn new(
        gateway: Gateway,
        session: SessionCapability,
        credential_store: Arc<dyn CredentialStore>,
    ) -> Self {
        Self {
            gateway,
            session,
            credential_store,
        }
    }

    pub async fn handle(&self, mut stream: UnixStream) {
        let first = match read_open_frame(&mut stream).await {
            Ok(first) => first,
            Err(_) => return,
        };
        let flow_id = first.flow_id();
        let prepared = match self.prepare_flow(first).await {
            Ok(value) => value,
            Err(error) => {
                let _error_result = send_setup_error(&mut stream, flow_id, error.code()).await;
                return;
            }
        };
        let owned_stream = match take_stream(&mut stream) {
            Ok(value) => value,
            Err(error) => {
                let _error_result = send_setup_error(&mut stream, flow_id, error.code()).await;
                return;
            }
        };
        match prepared.frame_type {
            FrameType::OpenTcp => {
                let _relay_result = relay_tcp(
                    owned_stream,
                    flow_id,
                    prepared.sequence,
                    prepared.initial_window_bytes,
                    prepared.connector,
                    &prepared.endpoint,
                )
                .await;
            }
            FrameType::OpenUdp => {
                let _relay_result = relay_udp(
                    owned_stream,
                    flow_id,
                    prepared.sequence,
                    prepared.initial_window_bytes,
                    prepared.connector,
                )
                .await;
            }
            _ => {}
        }
    }

    async fn prepare_flow(&self, first: FlowFrame) -> Result<PreparedFlow, FlowServiceError> {
        let mut sequence = SequenceTracker::default();
        sequence.accept(first.sequence())?;
        let frame_type = first.frame_type();
        if !matches!(frame_type, FrameType::OpenTcp | FrameType::OpenUdp) {
            return Err(nonproxy_flow_protocol::FlowProtocolError::InvalidFrameType.into());
        }
        let open = OpenFlowRequest::decode(first.payload())?;
        if !self.session.matches_token(open.capability()) {
            return Err(FlowServiceError::Authentication);
        }
        let connector = load_connector(
            &self.gateway,
            Arc::clone(&self.credential_store),
            open.outbound_id(),
        )
        .await?;
        if frame_type == FrameType::OpenUdp && !connector.supports_udp() {
            return Err(FlowServiceError::OutboundUnsupported);
        }
        Ok(PreparedFlow {
            frame_type,
            sequence,
            endpoint: open.endpoint().clone(),
            initial_window_bytes: open.initial_window_bytes(),
            connector,
        })
    }
}

struct PreparedFlow {
    frame_type: FrameType,
    sequence: SequenceTracker,
    endpoint: nonproxy_flow_protocol::FlowEndpoint,
    initial_window_bytes: u32,
    connector: OutboundConnector,
}

async fn read_open_frame(stream: &mut UnixStream) -> Result<FlowFrame, FlowServiceError> {
    timeout(OPEN_FRAME_TIMEOUT, read_frame(stream))
        .await
        .map_err(|_| FlowServiceError::PeerClosed)?
        .map_err(FlowServiceError::from)
}

fn take_stream(stream: &mut UnixStream) -> Result<UnixStream, FlowServiceError> {
    let (replacement, peer) = UnixStream::pair()?;
    drop(peer);
    Ok(std::mem::replace(stream, replacement))
}

async fn send_setup_error(
    stream: &mut UnixStream,
    flow_id: FlowId,
    code: &str,
) -> Result<(), FlowServiceError> {
    let frame = FlowFrame::new(FrameType::Error, 0, flow_id, 0, code.as_bytes().to_vec())?;
    write_frame(stream, &frame).await?;
    Ok(())
}
