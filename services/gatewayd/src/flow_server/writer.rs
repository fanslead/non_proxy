use nonproxy_flow_protocol::{FlowFrame, FlowId, FrameType, write_frame};
use tokio::{
    net::unix::OwnedWriteHalf,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::FlowServiceError;

const WRITER_QUEUE_FRAMES: usize = 64;

#[derive(Clone)]
pub struct FlowFrameSender {
    sender: mpsc::Sender<WriteRequest>,
}

struct WriteRequest {
    frame_type: FrameType,
    payload: Vec<u8>,
    completion: oneshot::Sender<Result<(), FlowServiceError>>,
}

impl FlowFrameSender {
    pub fn start(
        writer: OwnedWriteHalf,
        flow_id: FlowId,
    ) -> (Self, JoinHandle<Result<(), FlowServiceError>>) {
        let (sender, receiver) = mpsc::channel(WRITER_QUEUE_FRAMES);
        let task = tokio::spawn(run_writer(writer, flow_id, receiver));
        (Self { sender }, task)
    }

    pub async fn send(
        &self,
        frame_type: FrameType,
        payload: Vec<u8>,
    ) -> Result<(), FlowServiceError> {
        let (completion, result) = oneshot::channel();
        self.sender
            .send(WriteRequest {
                frame_type,
                payload,
                completion,
            })
            .await
            .map_err(|_| FlowServiceError::ChannelClosed)?;
        result.await.map_err(|_| FlowServiceError::ChannelClosed)?
    }
}

async fn run_writer(
    mut writer: OwnedWriteHalf,
    flow_id: FlowId,
    mut receiver: mpsc::Receiver<WriteRequest>,
) -> Result<(), FlowServiceError> {
    let mut sequence = 0_u64;
    while let Some(request) = receiver.recv().await {
        let frame = FlowFrame::new(request.frame_type, 0, flow_id, sequence, request.payload)?;
        let result = write_frame(&mut writer, &frame)
            .await
            .map_err(FlowServiceError::from);
        let failed = result.is_err();
        let _completion_result = request.completion.send(result);
        if failed {
            return Err(FlowServiceError::ChannelClosed);
        }
        sequence = sequence
            .checked_add(1)
            .ok_or(nonproxy_flow_protocol::FlowProtocolError::SequenceExhausted)?;
    }
    Ok(())
}
