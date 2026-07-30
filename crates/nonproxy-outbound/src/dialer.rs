use std::{future::Future, pin::Pin};

use nonproxy_flow_protocol::FlowEndpoint;
use tokio::net::TcpStream;

use crate::OutboundError;

pub trait TcpDialer: Send + Sync {
    fn connect<'a>(
        &'a self,
        endpoint: &'a FlowEndpoint,
    ) -> Pin<Box<dyn Future<Output = Result<TcpStream, OutboundError>> + Send + 'a>>;
}

#[derive(Default)]
pub struct SystemTcpDialer;

impl TcpDialer for SystemTcpDialer {
    fn connect<'a>(
        &'a self,
        endpoint: &'a FlowEndpoint,
    ) -> Pin<Box<dyn Future<Output = Result<TcpStream, OutboundError>> + Send + 'a>> {
        Box::pin(async move {
            TcpStream::connect((endpoint.host(), endpoint.port()))
                .await
                .map_err(OutboundError::from)
        })
    }
}
