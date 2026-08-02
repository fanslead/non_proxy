mod connection;
mod error;
mod initial_ready;
pub(crate) mod outbound_factory;
mod tcp;
mod udp;
mod window;
mod writer;

use std::sync::Arc;

pub use connection::FlowConnectionHandler;
pub use error::FlowServiceError;
use initial_ready::send_initial_ready;
use outbound_factory::load_connector;
use tcp::relay_tcp;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Semaphore, watch},
    task::JoinSet,
};
use tokio_stream::{Stream, StreamExt};
use udp::relay_udp;
use window::FlowWindow;
use writer::FlowFrameSender;

const MAXIMUM_ACTIVE_FLOWS: usize = 2_048;

pub trait FlowTransport: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

impl<T> FlowTransport for T where T: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

pub type BoxedFlowTransport = Box<dyn FlowTransport>;

pub struct FlowServer {
    handler: FlowConnectionHandler,
    capacity: Arc<Semaphore>,
}

impl FlowServer {
    pub fn new(handler: FlowConnectionHandler) -> Self {
        Self {
            handler,
            capacity: Arc::new(Semaphore::new(MAXIMUM_ACTIVE_FLOWS)),
        }
    }

    #[cfg(all(test, unix))]
    fn with_maximum_active_flows(handler: FlowConnectionHandler, maximum: usize) -> Self {
        Self {
            handler,
            capacity: Arc::new(Semaphore::new(maximum)),
        }
    }

    pub async fn serve<S, T>(
        self,
        mut incoming: S,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), std::io::Error>
    where
        S: Stream<Item = Result<T, std::io::Error>> + Unpin,
        T: FlowTransport,
    {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                accepted = incoming.next() => {
                    let Some(stream) = accepted else {
                        break;
                    };
                    let stream = stream?;
                    let Ok(permit) = Arc::clone(&self.capacity).try_acquire_owned() else {
                        drop(stream);
                        continue;
                    };
                    let handler = self.handler.clone();
                    tasks.spawn(async move {
                        let _permit = permit;
                        handler.handle(stream).await;
                    });
                }
                completed = tasks.join_next(), if !tasks.is_empty() => {
                    let _completed_result = completed;
                }
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod group_tests;
#[cfg(all(test, unix))]
mod tests;
#[cfg(all(test, unix))]
mod udp_tests;
