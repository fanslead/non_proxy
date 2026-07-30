use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use nonproxy_windows_ipc::SecureNamedPipeFactory;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::transport::server::Connected;

const INCOMING_QUEUE_CAPACITY: usize = 64;

pub struct ConnectedNamedPipe {
    inner: NamedPipeServer,
}

impl Connected for ConnectedNamedPipe {
    type ConnectInfo = ();

    fn connect_info(&self) -> Self::ConnectInfo {}
}

impl AsyncRead for ConnectedNamedPipe {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}

impl AsyncWrite for ConnectedNamedPipe {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.inner).poll_write(context, buffer)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

pub struct NamedPipeIncoming {
    receiver: ReceiverStream<io::Result<ConnectedNamedPipe>>,
    accept_task: JoinHandle<()>,
}

impl NamedPipeIncoming {
    pub fn bind(
        pipe_name: &str,
        pipe_sddl: &str,
        maximum_instances: usize,
        shutdown: watch::Receiver<bool>,
    ) -> io::Result<Self> {
        if !(1..=254).contains(&maximum_instances) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "命名管道实例上限必须位于 1 到 254",
            ));
        }
        let factory = SecureNamedPipeFactory::new(pipe_sddl)?;
        let first = create_instance(&factory, pipe_name, maximum_instances, true)?;
        let (sender, receiver) = mpsc::channel(INCOMING_QUEUE_CAPACITY);
        let name = pipe_name.to_owned();
        let accept_task = tokio::spawn(accept_loop(
            factory,
            name,
            maximum_instances,
            first,
            sender,
            shutdown,
        ));
        Ok(Self {
            receiver: ReceiverStream::new(receiver),
            accept_task,
        })
    }
}

impl Stream for NamedPipeIncoming {
    type Item = io::Result<ConnectedNamedPipe>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_next(context)
    }
}

impl Drop for NamedPipeIncoming {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

async fn accept_loop(
    factory: SecureNamedPipeFactory,
    pipe_name: String,
    maximum_instances: usize,
    mut current: NamedPipeServer,
    sender: mpsc::Sender<io::Result<ConnectedNamedPipe>>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
            connected = current.connect() => {
                if let Err(error) = connected {
                    let _send_result = sender.send(Err(error)).await;
                    return;
                }
            }
        }
        let next = match create_instance(&factory, &pipe_name, maximum_instances, false) {
            Ok(value) => value,
            Err(error) => {
                let _send_result = sender.send(Err(error)).await;
                return;
            }
        };
        if sender
            .send(Ok(ConnectedNamedPipe { inner: current }))
            .await
            .is_err()
        {
            return;
        }
        current = next;
    }
}

fn create_instance(
    factory: &SecureNamedPipeFactory,
    pipe_name: &str,
    maximum_instances: usize,
    first: bool,
) -> io::Result<NamedPipeServer> {
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .max_instances(maximum_instances);
    factory.create(&options, pipe_name)
}

#[cfg(test)]
mod tests {
    use std::io;

    use tokio::sync::watch;

    use super::NamedPipeIncoming;

    #[test]
    fn rejects_instance_limits_outside_windows_range_before_binding() {
        for maximum_instances in [0, 255] {
            let (_shutdown_sender, shutdown_receiver) = watch::channel(false);
            let incoming = NamedPipeIncoming::bind(
                r"\\.\pipe\NonProxy.Test.Flow",
                "D:P(A;;GA;;;SY)",
                maximum_instances,
                shutdown_receiver,
            );
            let Err(error) = incoming else {
                panic!("越界实例上限不应创建命名管道监听器");
            };

            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }
}
