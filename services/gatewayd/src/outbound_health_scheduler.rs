use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use nonproxy_model::OutboundId;
use nonproxy_storage::OutboundReference;
use tokio::{
    sync::watch,
    task::JoinSet,
    time::{Instant, MissedTickBehavior, interval},
};

use crate::{
    Gateway, credential_store::CredentialStore, outbound_probe_runner,
    outbound_probe_runner::DEFAULT_PROBE_TIMEOUT,
};

const SCAN_INTERVAL: Duration = Duration::from_secs(1);
const PROBE_INTERVAL: Duration = Duration::from_secs(20);
const PROBE_JITTER_WINDOW: Duration = Duration::from_secs(10);
const MAX_PARALLEL_PROBES: usize = 4;

type ProbeCompletion = (OutboundId, u64, Instant);

#[derive(Clone)]
pub(crate) struct OutboundHealthScheduler {
    gateway: Gateway,
    credential_store: Arc<dyn CredentialStore>,
}

#[derive(Clone, Copy)]
struct ScheduledProbe {
    revision: u64,
    next_due: Instant,
}

impl OutboundHealthScheduler {
    pub(crate) fn new(gateway: Gateway, credential_store: Arc<dyn CredentialStore>) -> Self {
        Self {
            gateway,
            credential_store,
        }
    }

    pub(crate) async fn serve(self, mut shutdown: watch::Receiver<bool>) {
        if *shutdown.borrow() {
            return;
        }
        let mut ticker = interval(SCAN_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut tasks = JoinSet::<ProbeCompletion>::new();
        let mut active = HashSet::new();
        let mut schedule = HashMap::new();

        loop {
            tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                completion = tasks.join_next(), if !tasks.is_empty() => {
                    complete_probe(completion, &mut tasks, &mut active, &mut schedule);
                }
                _ = ticker.tick(), if tasks.len() < MAX_PARALLEL_PROBES => {
                    self.schedule_due(&mut tasks, &mut active, &mut schedule).await;
                }
            }
        }

        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }

    async fn schedule_due(
        &self,
        tasks: &mut JoinSet<ProbeCompletion>,
        active: &mut HashSet<OutboundId>,
        schedule: &mut HashMap<OutboundId, ScheduledProbe>,
    ) {
        let Ok(outbounds) = self.gateway.list_outbounds().await else {
            return;
        };
        let current_revisions = outbounds
            .iter()
            .filter(|outbound| outbound.enabled())
            .map(|outbound| (outbound.id().clone(), outbound.revision()))
            .collect::<HashMap<_, _>>();
        schedule.retain(|outbound_id, entry| {
            current_revisions.get(outbound_id) == Some(&entry.revision)
        });
        let available = MAX_PARALLEL_PROBES.saturating_sub(tasks.len());
        let now = Instant::now();
        for outbound in select_due(outbounds, active, schedule, now, available) {
            let outbound_id = outbound.id().clone();
            let revision = outbound.revision();
            active.insert(outbound_id.clone());
            let gateway = self.gateway.clone();
            let credential_store = Arc::clone(&self.credential_store);
            tasks.spawn(async move {
                let _outcome = outbound_probe_runner::run(
                    &gateway,
                    credential_store,
                    outbound,
                    DEFAULT_PROBE_TIMEOUT,
                )
                .await;
                (outbound_id, revision, Instant::now())
            });
        }
    }
}

fn select_due(
    outbounds: Vec<OutboundReference>,
    active: &HashSet<OutboundId>,
    schedule: &HashMap<OutboundId, ScheduledProbe>,
    now: Instant,
    limit: usize,
) -> Vec<OutboundReference> {
    outbounds
        .into_iter()
        .filter(|outbound| outbound.enabled() && !active.contains(outbound.id()))
        .filter(|outbound| {
            schedule
                .get(outbound.id())
                .is_none_or(|entry| entry.revision != outbound.revision() || entry.next_due <= now)
        })
        .take(limit)
        .collect()
}

fn complete_probe(
    completion: Option<Result<ProbeCompletion, tokio::task::JoinError>>,
    tasks: &mut JoinSet<ProbeCompletion>,
    active: &mut HashSet<OutboundId>,
    schedule: &mut HashMap<OutboundId, ScheduledProbe>,
) {
    match completion {
        Some(Ok((outbound_id, revision, completed_at))) => {
            active.remove(&outbound_id);
            let next_due = completed_at + PROBE_INTERVAL + probe_jitter(&outbound_id, revision);
            schedule.insert(outbound_id, ScheduledProbe { revision, next_due });
        }
        Some(Err(_)) => {
            tasks.abort_all();
            active.clear();
        }
        None => {}
    }
}

fn probe_jitter(outbound_id: &OutboundId, revision: u64) -> Duration {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in outbound_id.as_str().bytes().chain(revision.to_be_bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let window_ms = u64::try_from(PROBE_JITTER_WINDOW.as_millis()).unwrap_or(u64::MAX);
    Duration::from_millis(hash % window_ms.max(1))
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use nonproxy_model::OutboundId;
    use nonproxy_storage::{OutboundKind, OutboundReference};
    use tokio::time::{Duration, Instant};

    use super::{
        MAX_PARALLEL_PROBES, PROBE_INTERVAL, PROBE_JITTER_WINDOW, ScheduledProbe, probe_jitter,
        select_due,
    };

    #[test]
    fn new_enabled_outbounds_are_immediately_due_with_a_parallel_bound() {
        let now = Instant::now();
        let outbounds = (0..6)
            .map(|index| outbound(&format!("proxy-{index}"), 1))
            .collect();

        let selected = select_due(
            outbounds,
            &HashSet::new(),
            &HashMap::new(),
            now,
            MAX_PARALLEL_PROBES,
        );

        assert_eq!(selected.len(), MAX_PARALLEL_PROBES);
        assert_eq!(selected[0].id().as_str(), "proxy-0");
    }

    #[test]
    fn future_schedule_is_respected_but_a_revision_change_is_immediate() {
        let now = Instant::now();
        let id = outbound_id("primary");
        let schedule = HashMap::from([(
            id,
            ScheduledProbe {
                revision: 1,
                next_due: now + Duration::from_secs(60),
            },
        )]);

        assert!(
            select_due(
                vec![outbound("primary", 1)],
                &HashSet::new(),
                &schedule,
                now,
                1,
            )
            .is_empty()
        );
        assert_eq!(
            select_due(
                vec![outbound("primary", 2)],
                &HashSet::new(),
                &schedule,
                now,
                1,
            )
            .len(),
            1
        );
    }

    #[test]
    fn deterministic_jitter_stays_inside_the_configured_window() {
        let id = outbound_id("primary");
        let jitter = probe_jitter(&id, 7);

        assert_eq!(jitter, probe_jitter(&id, 7));
        assert!(jitter < PROBE_JITTER_WINDOW);
        assert!(PROBE_INTERVAL + jitter < Duration::from_secs(30));
    }

    fn outbound(id: &str, revision: u64) -> OutboundReference {
        OutboundReference::new(
            outbound_id(id),
            OutboundKind::Socks5,
            Some("proxy.example"),
            Some(1_080),
            None,
            revision,
        )
        .unwrap_or_else(|error| panic!("测试出口创建失败: {error}"))
    }

    fn outbound_id(value: &str) -> OutboundId {
        OutboundId::new(value).unwrap_or_else(|error| panic!("测试出口 ID 无效: {error}"))
    }
}
