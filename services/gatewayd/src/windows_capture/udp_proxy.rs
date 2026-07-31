use std::{collections::HashMap, sync::Arc};

use nonproxy_windows_wfp::{UdpDatagram, WfpDriver};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, watch},
    task::JoinSet,
};

use crate::GatewayError;

use super::{
    udp_driver::UdpDriverPump,
    udp_session::{PendingUdpPayload, UdpSessionDependencies, run_udp_session},
};

const MAXIMUM_ACTIVE_SESSIONS: usize = 2_048;
const SESSION_QUEUE_CAPACITY: usize = 64;
const MAXIMUM_PENDING_SESSION_BYTES: usize = 32 * 1024 * 1024;

pub struct WindowsUdpProxy {
    pump: UdpDriverPump,
    dependencies: UdpSessionDependencies,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct UdpSessionKey {
    process_id: u64,
    local: std::net::SocketAddr,
    remote: std::net::SocketAddr,
    app_id: Vec<u8>,
}

impl UdpSessionKey {
    fn from_datagram(datagram: &UdpDatagram) -> Self {
        Self {
            process_id: datagram.process_id(),
            local: datagram.local(),
            remote: datagram.remote(),
            app_id: datagram.app_id().to_vec(),
        }
    }
}

impl WindowsUdpProxy {
    pub fn start(
        driver: Arc<WfpDriver>,
        dependencies: impl FnOnce(super::udp_driver::UdpInjector) -> UdpSessionDependencies,
    ) -> Self {
        let pump = UdpDriverPump::start(driver);
        let session_dependencies = dependencies(pump.injector());
        Self {
            pump,
            dependencies: session_dependencies,
        }
    }

    pub async fn serve(self, mut shutdown: watch::Receiver<bool>) -> Result<(), GatewayError> {
        let Self {
            mut pump,
            dependencies,
        } = self;
        let mut sessions = HashMap::<UdpSessionKey, mpsc::Sender<PendingUdpPayload>>::new();
        let mut tasks = JoinSet::<UdpSessionKey>::new();
        let pending_budget = Arc::new(Semaphore::new(MAXIMUM_PENDING_SESSION_BYTES));
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                datagram = pump.receive() => {
                    match datagram {
                        Some(datagram) => dispatch(
                            datagram,
                            &mut sessions,
                            &mut tasks,
                            &dependencies,
                            &pending_budget,
                        ),
                        None => break,
                    }
                }
                completed = tasks.join_next(), if !tasks.is_empty() => {
                    if let Some(Ok(key)) = completed {
                        sessions.remove(&key);
                    }
                }
            }
        }

        sessions.clear();
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        pump.shutdown().await
    }
}

fn dispatch(
    datagram: UdpDatagram,
    sessions: &mut HashMap<UdpSessionKey, mpsc::Sender<PendingUdpPayload>>,
    tasks: &mut JoinSet<UdpSessionKey>,
    dependencies: &UdpSessionDependencies,
    pending_budget: &Arc<Semaphore>,
) {
    let key = UdpSessionKey::from_datagram(&datagram);
    if let Some(sender) = sessions.get(&key) {
        let Some(permit) = acquire_budget(pending_budget, datagram.payload().len()) else {
            return;
        };
        let payload = PendingUdpPayload::new(datagram.payload().to_vec(), permit);
        match sender.try_send(payload) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => return,
            Err(mpsc::error::TrySendError::Closed(_)) => {
                sessions.remove(&key);
            }
        }
    }
    if sessions.len() >= MAXIMUM_ACTIVE_SESSIONS {
        return;
    }
    let Some(permit) = acquire_budget(pending_budget, datagram.payload().len()) else {
        return;
    };
    let (sender, receiver) = mpsc::channel(SESSION_QUEUE_CAPACITY);
    sessions.insert(key.clone(), sender);
    let task_key = key;
    let task_dependencies = dependencies.clone();
    tasks.spawn(async move {
        let _result = run_udp_session(datagram, permit, receiver, task_dependencies).await;
        task_key
    });
}

fn acquire_budget(budget: &Arc<Semaphore>, bytes: usize) -> Option<OwnedSemaphorePermit> {
    let permits = u32::try_from(bytes.max(1)).ok()?;
    Arc::clone(budget).try_acquire_many_owned(permits).ok()
}
