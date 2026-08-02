use std::{sync::Arc, time::Duration};

use nonproxy_flow_protocol::{
    FlowId, FlowProtocolError, FrameType, SequenceTracker, WindowUpdate, read_frame,
};
use nonproxy_model::OutboundId;
use nonproxy_outbound::{BoxedProxyStream, OutboundConnector};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    time::timeout,
};

use super::{
    BoxedFlowTransport, FlowFrameSender, FlowServiceError, FlowWindow, send_initial_ready,
};

const SERVER_RECEIVE_WINDOW_BYTES: u32 = 256 * 1024;
const MAXIMUM_READ_BYTES: usize = 64 * 1024;
const REMOTE_HALF_CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn relay_tcp(
    stream: BoxedFlowTransport,
    flow_id: FlowId,
    sequence: SequenceTracker,
    client_window_bytes: u32,
    connector: OutboundConnector,
    target: &nonproxy_flow_protocol::FlowEndpoint,
    selected_outbound: Option<&OutboundId>,
) -> Result<(), FlowServiceError> {
    let proxy = connector.connect_tcp(target).await?;
    let (reader, writer) = tokio::io::split(stream);
    let (proxy_reader, proxy_writer) = tokio::io::split(proxy);
    let (sender, writer_task) = FlowFrameSender::start(writer, flow_id);
    send_initial_ready(&sender, selected_outbound, SERVER_RECEIVE_WINDOW_BYTES).await?;
    let client_window = Arc::new(FlowWindow::new(client_window_bytes)?);
    let relay_result = run_pumps(
        reader,
        proxy_reader,
        proxy_writer,
        flow_id,
        sequence,
        Arc::clone(&client_window),
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
    proxy_reader: ReadHalf<BoxedProxyStream>,
    proxy_writer: WriteHalf<BoxedProxyStream>,
    flow_id: FlowId,
    sequence: SequenceTracker,
    client_window: Arc<FlowWindow>,
    sender: FlowFrameSender,
) -> Result<(), FlowServiceError> {
    let upstream = client_to_proxy(
        reader,
        proxy_writer,
        flow_id,
        sequence,
        Arc::clone(&client_window),
        sender.clone(),
    );
    let downstream = proxy_to_client(proxy_reader, client_window, sender);
    tokio::pin!(upstream);
    tokio::pin!(downstream);
    tokio::select! {
        result = &mut upstream => result,
        result = &mut downstream => {
            result?;
            timeout(REMOTE_HALF_CLOSE_TIMEOUT, &mut upstream)
                .await
                .map_err(|_| FlowServiceError::PeerClosed)?
        }
    }
}

async fn client_to_proxy(
    mut reader: ReadHalf<BoxedFlowTransport>,
    mut proxy_writer: WriteHalf<BoxedProxyStream>,
    flow_id: FlowId,
    mut sequence: SequenceTracker,
    client_window: Arc<FlowWindow>,
    sender: FlowFrameSender,
) -> Result<(), FlowServiceError> {
    let mut receive_window = u64::from(SERVER_RECEIVE_WINDOW_BYTES);
    let mut app_half_closed = false;
    loop {
        let frame = read_frame(&mut reader).await?;
        validate_frame(&frame, flow_id, &mut sequence)?;
        match frame.frame_type() {
            FrameType::Data if !app_half_closed => {
                let bytes = frame.payload().len();
                if bytes == 0 || bytes as u64 > receive_window {
                    return Err(FlowServiceError::InvalidWindow);
                }
                receive_window -= bytes as u64;
                proxy_writer.write_all(frame.payload()).await?;
                let update = WindowUpdate::new(
                    u32::try_from(bytes).map_err(|_| FlowServiceError::InvalidWindow)?,
                )?;
                sender
                    .send(FrameType::WindowUpdate, update.encode().to_vec())
                    .await?;
                receive_window += bytes as u64;
            }
            FrameType::HalfClose if !app_half_closed => {
                app_half_closed = true;
                proxy_writer.shutdown().await?;
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
    mut proxy_reader: ReadHalf<BoxedProxyStream>,
    client_window: Arc<FlowWindow>,
    sender: FlowFrameSender,
) -> Result<(), FlowServiceError> {
    loop {
        let allowance = client_window.take_up_to(MAXIMUM_READ_BYTES).await?;
        let mut buffer = vec![0_u8; allowance];
        let read = proxy_reader.read(&mut buffer).await?;
        if read < allowance {
            client_window.refund(allowance - read).await?;
        }
        if read == 0 {
            sender.send(FrameType::HalfClose, Vec::new()).await?;
            return Ok(());
        }
        buffer.truncate(read);
        sender.send(FrameType::Data, buffer).await?;
    }
}

fn validate_frame(
    frame: &nonproxy_flow_protocol::FlowFrame,
    flow_id: FlowId,
    sequence: &mut SequenceTracker,
) -> Result<(), FlowServiceError> {
    if frame.flow_id() != flow_id {
        return Err(FlowProtocolError::InvalidFlowId.into());
    }
    sequence.accept(frame.sequence())?;
    Ok(())
}
