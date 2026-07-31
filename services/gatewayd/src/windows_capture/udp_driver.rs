use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel},
    },
    time::Duration,
};

use nonproxy_windows_wfp::{
    UdpDatagram, UdpInjection, UdpInjectionContext, WfpDriver, WindowsWfpError,
};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::GatewayError;

const DRIVER_POLL_INTERVAL: Duration = Duration::from_millis(2);
const DATAGRAM_CHANNEL_CAPACITY: usize = 256;
const INJECTION_CHANNEL_CAPACITY: usize = 256;
const MAXIMUM_INJECTIONS_PER_POLL: usize = 128;

pub struct UdpDriverPump {
    incoming: mpsc::Receiver<UdpDatagram>,
    injector: UdpInjector,
    stop: Arc<AtomicBool>,
    task: JoinHandle<Result<(), GatewayError>>,
}

#[derive(Clone)]
pub struct UdpInjector {
    sender: SyncSender<UdpInjection>,
}

impl UdpInjector {
    pub fn inject(&self, context: UdpInjectionContext, payload: &[u8]) -> Result<(), GatewayError> {
        let injection = UdpInjection::encode(context, payload).map_err(data_plane_error)?;
        self.sender
            .try_send(injection)
            .map_err(|error| match error {
                TrySendError::Full(_) => {
                    GatewayError::WindowsDataPlane("Windows UDP 注入队列已满".to_owned())
                }
                TrySendError::Disconnected(_) => {
                    GatewayError::WindowsDataPlane("Windows UDP 注入通道已关闭".to_owned())
                }
            })
    }
}

impl UdpDriverPump {
    pub fn start(driver: Arc<WfpDriver>) -> Self {
        let (incoming_sender, incoming) = mpsc::channel(DATAGRAM_CHANNEL_CAPACITY);
        let (injection_sender, injection_receiver) = sync_channel(INJECTION_CHANNEL_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let task = tokio::task::spawn_blocking(move || {
            run_driver_pump(driver, incoming_sender, injection_receiver, worker_stop)
        });
        Self {
            incoming,
            injector: UdpInjector {
                sender: injection_sender,
            },
            stop,
            task,
        }
    }

    #[must_use]
    pub fn injector(&self) -> UdpInjector {
        self.injector.clone()
    }

    pub async fn receive(&mut self) -> Option<UdpDatagram> {
        self.incoming.recv().await
    }

    pub async fn shutdown(self) -> Result<(), GatewayError> {
        self.stop.store(true, Ordering::Release);
        self.task.await.map_err(|_| {
            GatewayError::WindowsDataPlane("Windows UDP 驱动任务异常退出".to_owned())
        })?
    }
}

fn run_driver_pump(
    driver: Arc<WfpDriver>,
    incoming: mpsc::Sender<UdpDatagram>,
    injections: Receiver<UdpInjection>,
    stop: Arc<AtomicBool>,
) -> Result<(), GatewayError> {
    while !stop.load(Ordering::Acquire) {
        let mut did_work = false;
        for _ in 0..MAXIMUM_INJECTIONS_PER_POLL {
            match injections.try_recv() {
                Ok(injection) => {
                    if let Err(error) = driver.inject_udp(&injection) {
                        if stop.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        return Err(data_plane_error(error));
                    }
                    did_work = true;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        let datagrams = driver.receive_udp_batch().map_err(data_plane_error)?;
        if !datagrams.is_empty() {
            did_work = true;
        }
        for datagram in datagrams {
            if incoming.blocking_send(datagram).is_err() {
                return Ok(());
            }
        }
        if !did_work {
            std::thread::sleep(DRIVER_POLL_INTERVAL);
        }
    }
    Ok(())
}

fn data_plane_error(error: WindowsWfpError) -> GatewayError {
    GatewayError::WindowsDataPlane(error.to_string())
}
