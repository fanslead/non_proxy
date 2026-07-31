use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nonproxy_model::{ConnectionContext, Decision, OutboundId};
use nonproxy_storage::{ConnectionDecisionInput, DecisionEvidence, EvidenceLevel};
use tokio::{
    sync::{mpsc, watch},
    time::sleep,
};

use crate::{Gateway, GatewayError};

const EVENT_QUEUE_CAPACITY: usize = 4_096;
const MAXIMUM_BATCH_SIZE: usize = 128;
const RETRY_DELAY: Duration = Duration::from_secs(1);
static NEXT_FLOW_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn new_flow_id(prefix: &str) -> String {
    let sequence = NEXT_FLOW_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{sequence:032x}")
}

pub(crate) fn elapsed_micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).map_or(u64::MAX, |value| value)
}

pub(crate) enum ObservedPath {
    Decision,
    Direct {
        interface_index: u32,
        fail_open: bool,
    },
    Proxy {
        outbound_id: OutboundId,
    },
}

pub(crate) fn observed_dns_path(cache_hit: bool, established_path: ObservedPath) -> ObservedPath {
    if cache_hit {
        ObservedPath::Decision
    } else {
        established_path
    }
}

pub(crate) struct DecisionObservation {
    provider_id: &'static str,
    provider_generation: u64,
    flow_id: String,
    occurred_at_unix_ms: u64,
    context: ConnectionContext,
    decision: Decision,
    decision_latency_micros: u64,
}

impl DecisionObservation {
    pub(crate) fn new(
        provider_id: &'static str,
        provider_generation: u64,
        flow_id: String,
        occurred_at_unix_ms: u64,
        context: ConnectionContext,
        decision: Decision,
        decision_latency_micros: u64,
    ) -> Self {
        Self {
            provider_id,
            provider_generation,
            flow_id,
            occurred_at_unix_ms,
            context,
            decision,
            decision_latency_micros,
        }
    }

    pub(crate) fn record(
        &self,
        path: ObservedPath,
        error_code: Option<&str>,
    ) -> Result<ConnectionDecisionInput, GatewayError> {
        let evidence = match path {
            ObservedPath::Decision => {
                DecisionEvidence::new(EvidenceLevel::Decision, None, None, None, false)?
            }
            ObservedPath::Direct {
                interface_index,
                fail_open,
            } => {
                if interface_index == 0 {
                    return Err(GatewayError::InvalidContract(
                        "Windows DIRECT 路径缺少物理接口",
                    ));
                }
                DecisionEvidence::new(
                    EvidenceLevel::Path,
                    Some(format!("ifindex:{interface_index}")),
                    None,
                    None,
                    fail_open,
                )?
            }
            ObservedPath::Proxy { outbound_id } => {
                DecisionEvidence::new(EvidenceLevel::Path, None, Some(outbound_id), None, false)?
            }
        };
        ConnectionDecisionInput::new(
            self.provider_id,
            self.provider_generation,
            self.flow_id.clone(),
            self.occurred_at_unix_ms,
            self.context.app().clone(),
            self.context.destination().clone(),
            self.decision.clone(),
            evidence,
            Some(self.decision_latency_micros),
            error_code.map(str::to_owned),
        )
        .map_err(GatewayError::from)
    }
}

#[derive(Clone)]
pub(crate) struct DecisionEventReporter {
    sender: mpsc::Sender<ConnectionDecisionInput>,
    dropped_events: Arc<AtomicU64>,
    gateway: Gateway,
}

pub(crate) struct DecisionEventWorker {
    gateway: Gateway,
    receiver: mpsc::Receiver<ConnectionDecisionInput>,
}

pub(crate) fn decision_event_channel(
    gateway: Gateway,
) -> (DecisionEventReporter, DecisionEventWorker) {
    let (sender, receiver) = mpsc::channel(EVENT_QUEUE_CAPACITY);
    (
        DecisionEventReporter {
            sender,
            dropped_events: Arc::new(AtomicU64::new(0)),
            gateway: gateway.clone(),
        },
        DecisionEventWorker { gateway, receiver },
    )
}

impl DecisionEventReporter {
    pub(crate) fn submit(&self, decision: ConnectionDecisionInput) -> bool {
        match self.sender.try_send(decision) {
            Ok(()) => true,
            Err(_) => {
                self.record_unreportable();
                false
            }
        }
    }

    pub(crate) fn record_unreportable(&self) {
        self.gateway.record_dropped_decisions(1);
        let _previous =
            self.dropped_events
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    Some(value.saturating_add(1))
                });
    }

    #[cfg(test)]
    fn dropped_events(&self) -> u64 {
        self.dropped_events.load(Ordering::Relaxed)
    }
}

impl DecisionEventWorker {
    pub(crate) async fn serve(
        mut self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), GatewayError> {
        let mut stopping = *shutdown.borrow();
        let mut batch = Vec::with_capacity(MAXIMUM_BATCH_SIZE);
        loop {
            if batch.is_empty() {
                let next = if stopping {
                    self.receiver.recv().await
                } else {
                    tokio::select! {
                        changed = shutdown.changed() => {
                            let _changed = changed;
                            stopping = true;
                            continue;
                        }
                        received = self.receiver.recv() => received,
                    }
                };
                let Some(decision) = next else {
                    return Ok(());
                };
                batch.push(decision);
                while batch.len() < MAXIMUM_BATCH_SIZE {
                    match self.receiver.try_recv() {
                        Ok(decision) => batch.push(decision),
                        Err(_) => break,
                    }
                }
            }
            match self.gateway.store_connection_decisions(batch.clone()).await {
                Ok(()) => batch.clear(),
                Err(error) if stopping => return Err(error),
                Err(_) => {
                    tokio::select! {
                        () = sleep(RETRY_DELAY) => {}
                        changed = shutdown.changed() => {
                            let _changed = changed;
                            stopping = true;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use nonproxy_model::{
        AppIdentity, DecisionSpec, Destination, FailureMode, Platform, RouteAction, Transport,
    };
    use nonproxy_policy_compiler::CompileCapabilities;
    use nonproxy_proto::{common::v1::ComponentKind, events::v1::event_envelope};
    use nonproxy_storage::PolicyDatabase;

    use super::*;

    #[test]
    fn fail_open_record_uses_the_observed_direct_interface() {
        let observation = sample_observation(FailureMode::Open);
        let record = observation.record(
            ObservedPath::Direct {
                interface_index: 12,
                fail_open: true,
            },
            Some("NP_WINDOWS_PROXY_FAIL_OPEN_DIRECT"),
        );

        assert!(record.is_ok());
    }

    #[test]
    fn closed_proxy_cannot_claim_a_fail_open_path() {
        let observation = sample_observation(FailureMode::Closed);
        let record = observation.record(
            ObservedPath::Direct {
                interface_index: 12,
                fail_open: true,
            },
            Some("NP_WINDOWS_PROXY_FAIL_OPEN_DIRECT"),
        );

        assert!(record.is_err());
    }

    #[test]
    fn cached_dns_answer_does_not_claim_a_new_network_path() {
        assert!(matches!(
            observed_dns_path(
                true,
                ObservedPath::Direct {
                    interface_index: 12,
                    fail_open: false,
                },
            ),
            ObservedPath::Decision
        ));
        assert!(matches!(
            observed_dns_path(
                false,
                ObservedPath::Direct {
                    interface_index: 12,
                    fail_open: false,
                },
            ),
            ObservedPath::Direct {
                interface_index: 12,
                fail_open: false,
            }
        ));
    }

    #[tokio::test]
    async fn bounded_reporter_counts_events_it_cannot_queue() {
        let database = match PolicyDatabase::open_in_memory(1) {
            Ok(value) => value,
            Err(error) => panic!("决策队列测试数据库打开失败: {error}"),
        };
        let gateway = Gateway::new(database, CompileCapabilities::full());
        let (reporter, _worker) = decision_event_channel(gateway);
        let observation = sample_observation(FailureMode::Closed);

        for index in 0..EVENT_QUEUE_CAPACITY {
            let record = observation.record(ObservedPath::Decision, None);
            let Ok(record) = record else {
                panic!("第 {index} 条决策记录构造失败: {record:?}");
            };
            assert!(reporter.submit(record));
        }
        let overflow = observation.record(ObservedPath::Decision, None);
        let Ok(overflow) = overflow else {
            panic!("溢出决策记录构造失败: {overflow:?}");
        };
        assert!(!reporter.submit(overflow));
        assert_eq!(reporter.dropped_events(), 1);
    }

    #[tokio::test]
    async fn worker_flushes_a_proxy_path_when_producers_close() {
        let database = match PolicyDatabase::open_in_memory(1) {
            Ok(value) => value,
            Err(error) => panic!("决策 worker 测试数据库打开失败: {error}"),
        };
        let gateway = Gateway::new(database, CompileCapabilities::full());
        let query_gateway = gateway.clone();
        let (reporter, worker) = decision_event_channel(gateway);
        let outbound_id = match OutboundId::new("office") {
            Ok(value) => value,
            Err(error) => panic!("测试出口标识无效: {error}"),
        };
        let observation = sample_observation(FailureMode::Closed);
        let record = observation.record(ObservedPath::Proxy { outbound_id }, None);
        let Ok(record) = record else {
            panic!("代理路径记录构造失败: {record:?}");
        };
        assert!(reporter.submit(record));
        drop(reporter);
        let (_shutdown_sender, shutdown) = watch::channel(false);

        if let Err(error) = worker.serve(shutdown).await {
            panic!("决策 worker 刷盘失败: {error}");
        }
        let count = query_gateway
            .database
            .run(|database| {
                let (records, total) = database.connection_decisions().list_recent(10, 0)?;
                Ok((records.len(), total))
            })
            .await;
        assert_eq!(count.ok(), Some((1, 1)));
        let events = query_gateway.events().subscribe(0);
        let Ok((events, _receiver)) = events else {
            panic!("决策 worker 事件读取失败: {events:?}");
        };
        assert!(matches!(
            events.as_slice(),
            [event]
                if event.component == ComponentKind::WindowsService as i32
                    && matches!(
                        event.payload.as_ref(),
                        Some(event_envelope::Payload::DecisionObserved(observed))
                            if observed.flow_id == "flow-1"
                                && observed.decision.as_ref().is_some_and(|decision| {
                                    decision.result.as_ref().is_some_and(|result| {
                                        result.outbound_id == "office"
                                    })
                                })
                    )
        ));
    }

    #[test]
    fn flow_ids_are_prefixed_and_latency_is_bounded() {
        let flow_id = new_flow_id("tcp");
        assert!(flow_id.starts_with("tcp-") && flow_id.len() == 36);
        assert!(elapsed_micros(Instant::now()) < 1_000_000);
    }

    fn sample_observation(failure_mode: FailureMode) -> DecisionObservation {
        let outbound_id = match OutboundId::new("office") {
            Ok(value) => value,
            Err(error) => panic!("测试出口标识无效: {error}"),
        };
        let result = match DecisionSpec::new(RouteAction::Proxy, Some(outbound_id), failure_mode) {
            Ok(value) => value,
            Err(error) => panic!("测试决策无效: {error}"),
        };
        let context = ConnectionContext::new(
            match AppIdentity::new(Platform::Windows, "c:/browser.exe") {
                Ok(value) => value,
                Err(error) => panic!("测试应用身份无效: {error}"),
            },
            match Destination::new(
                None,
                Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 8))),
                443,
                Transport::Tcp,
            ) {
                Ok(value) => value,
                Err(error) => panic!("测试目标无效: {error}"),
            },
        );
        DecisionObservation::new(
            "windows-wfp",
            3,
            "flow-1".to_owned(),
            1_800_000_000_000,
            context,
            Decision::defaulted(result, 7, "NP_POLICY_DEFAULT"),
            125,
        )
    }
}
