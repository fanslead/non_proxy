use std::sync::Arc;

use nonproxy_flow_protocol::{
    DatagramPayload, FlowFrame, FlowId, FlowProtocolError, FrameType, SequenceTracker,
    WindowUpdate, read_frame,
};
use nonproxy_outbound::OutboundConnector;
use tokio::io::ReadHalf;

use super::{BoxedFlowTransport, FlowFrameSender, FlowServiceError, FlowWindow};

const SERVER_RECEIVE_WINDOW_BYTES: u32 = 256 * 1024;

pub async fn relay_udp(
    stream: BoxedFlowTransport,
    flow_id: FlowId,
    sequence: SequenceTracker,
    client_window_bytes: u32,
    connector: OutboundConnector,
) -> Result<(), FlowServiceError> {
    let association = Arc::new(connector.open_udp().await?);
    let (reader, writer) = tokio::io::split(stream);
    let (sender, writer_task) = FlowFrameSender::start(writer, flow_id);
    sender
        .send(
            FrameType::WindowUpdate,
            WindowUpdate::new(SERVER_RECEIVE_WINDOW_BYTES)?
                .encode()
                .to_vec(),
        )
        .await?;
    let client_window = Arc::new(FlowWindow::new(client_window_bytes)?);
    let relay_result = run_pumps(
        reader,
        flow_id,
        sequence,
        association,
        client_window,
        sender.clone(),
    )
    .await;
    let terminal_type = if relay_result.is_ok() {
        FrameType::Close
    } else {
        FrameType::Error
    };
    let terminal_payload = relay_result
        .as_ref()
        .err()
        .map_or_else(Vec::new, |error| error.code().as_bytes().to_vec());
    let _terminal_result = sender.send(terminal_type, terminal_payload).await;
    drop(sender);
    let writer_result = writer_task
        .await
        .map_err(|_| FlowServiceError::WriterTask)?;
    relay_result.and(writer_result)
}

async fn run_pumps(
    reader: ReadHalf<BoxedFlowTransport>,
    flow_id: FlowId,
    sequence: SequenceTracker,
    association: Arc<nonproxy_outbound::OutboundDatagramSession>,
    client_window: Arc<FlowWindow>,
    sender: FlowFrameSender,
) -> Result<(), FlowServiceError> {
    let upstream = client_to_proxy(
        reader,
        flow_id,
        sequence,
        Arc::clone(&association),
        Arc::clone(&client_window),
        sender.clone(),
    );
    let downstream = proxy_to_client(association, client_window, sender);
    tokio::pin!(upstream);
    tokio::pin!(downstream);
    tokio::select! {
        result = &mut upstream => result,
        result = &mut downstream => result,
    }
}

async fn client_to_proxy(
    mut reader: ReadHalf<BoxedFlowTransport>,
    flow_id: FlowId,
    mut sequence: SequenceTracker,
    association: Arc<nonproxy_outbound::OutboundDatagramSession>,
    client_window: Arc<FlowWindow>,
    sender: FlowFrameSender,
) -> Result<(), FlowServiceError> {
    let mut receive_window = u64::from(SERVER_RECEIVE_WINDOW_BYTES);
    loop {
        let frame = read_frame(&mut reader).await?;
        validate_frame(&frame, flow_id, &mut sequence)?;
        match frame.frame_type() {
            FrameType::Datagram => {
                let bytes = frame.payload().len();
                if bytes == 0 || bytes as u64 > receive_window {
                    return Err(FlowServiceError::InvalidWindow);
                }
                receive_window -= bytes as u64;
                let datagram = DatagramPayload::decode(frame.payload())?;
                association
                    .send(datagram.endpoint(), datagram.content())
                    .await?;
                let update = WindowUpdate::new(
                    u32::try_from(bytes).map_err(|_| FlowServiceError::InvalidWindow)?,
                )?;
                sender
                    .send(FrameType::WindowUpdate, update.encode().to_vec())
                    .await?;
                receive_window += bytes as u64;
            }
            FrameType::WindowUpdate => {
                client_window
                    .add(WindowUpdate::decode(frame.payload())?.bytes())
                    .await?;
            }
            FrameType::Ping => sender.send(FrameType::Pong, Vec::new()).await?,
            FrameType::Pong => {}
            FrameType::Close => return Ok(()),
            FrameType::Error => return Err(FlowServiceError::PeerClosed),
            _ => return Err(FlowProtocolError::InvalidFrameType.into()),
        }
    }
}

async fn proxy_to_client(
    association: Arc<nonproxy_outbound::OutboundDatagramSession>,
    client_window: Arc<FlowWindow>,
    sender: FlowFrameSender,
) -> Result<(), FlowServiceError> {
    loop {
        let (endpoint, content) = association.receive().await?;
        let payload = DatagramPayload::new(endpoint, content)?.encode()?;
        client_window.take_exact(payload.len()).await?;
        sender.send(FrameType::Datagram, payload).await?;
    }
}

fn validate_frame(
    frame: &FlowFrame,
    flow_id: FlowId,
    sequence: &mut SequenceTracker,
) -> Result<(), FlowServiceError> {
    if frame.flow_id() != flow_id {
        return Err(FlowProtocolError::InvalidFlowId.into());
    }
    sequence.accept(frame.sequence())?;
    Ok(())
}
