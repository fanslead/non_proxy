mod connection;
mod error;
mod outbound_factory;
mod tcp;
mod udp;
mod window;
mod writer;

use std::sync::Arc;

pub use connection::FlowConnectionHandler;
pub use error::FlowServiceError;
use outbound_factory::load_connector;
use tcp::relay_tcp;
use tokio::{
    net::UnixListener,
    sync::{Semaphore, watch},
    task::JoinSet,
};
use udp::relay_udp;
use window::FlowWindow;
use writer::FlowFrameSender;

const MAXIMUM_ACTIVE_FLOWS: usize = 2_048;

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

    #[cfg(test)]
    fn with_maximum_active_flows(handler: FlowConnectionHandler, maximum: usize) -> Self {
        Self {
            handler,
            capacity: Arc::new(Semaphore::new(maximum)),
        }
    }

    pub async fn serve(
        self,
        listener: UnixListener,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), std::io::Error> {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
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

#[cfg(test)]
mod tests;
#[cfg(test)]
mod udp_tests;
