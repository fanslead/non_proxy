use std::{collections::HashSet, time::Duration};

use nonproxy_storage::SubscriptionSource;
use tokio::{
    sync::watch,
    task::JoinSet,
    time::{MissedTickBehavior, interval},
};

use crate::{Gateway, clock::unix_time_ms, subscription_service::SubscriptionService};

const SCAN_INTERVAL: Duration = Duration::from_secs(30);
const MAX_PARALLEL_REFRESHES: usize = 4;
const MAX_DUE_SCAN: u32 = 100;

type RefreshCompletion = (String, ());

#[derive(Clone)]
pub(crate) struct SubscriptionScheduler {
    gateway: Gateway,
    service: SubscriptionService,
}

impl SubscriptionScheduler {
    pub(crate) fn new(gateway: Gateway, service: SubscriptionService) -> Self {
        Self { gateway, service }
    }

    pub(crate) async fn serve(self, mut shutdown: watch::Receiver<bool>) {
        if *shutdown.borrow() {
            self.service.close();
            self.service.wait_idle().await;
            return;
        }
        let mut ticker = interval(SCAN_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut tasks = JoinSet::<RefreshCompletion>::new();
        let mut active = HashSet::new();

        loop {
            tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                completion = tasks.join_next(), if !tasks.is_empty() => {
                    remove_completed(completion, &tasks, &mut active);
                }
                _ = ticker.tick(), if tasks.len() < MAX_PARALLEL_REFRESHES => {
                    if let Ok(now) = unix_time_ms() {
                        self.schedule_due(now, &mut tasks, &mut active).await;
                    }
                }
            }
        }

        self.service.close();
        while let Some(completion) = tasks.join_next().await {
            remove_completed(Some(completion), &tasks, &mut active);
        }
        self.service.wait_idle().await;
    }

    pub(crate) async fn schedule_due(
        &self,
        now_unix_ms: u64,
        tasks: &mut JoinSet<RefreshCompletion>,
        active: &mut HashSet<String>,
    ) {
        let available = MAX_PARALLEL_REFRESHES.saturating_sub(tasks.len());
        let Ok(sources) = self
            .gateway
            .due_subscription_sources(now_unix_ms, MAX_DUE_SCAN)
            .await
        else {
            return;
        };
        for source in select_new_sources(sources, active, available) {
            let source_id = source.id().to_owned();
            active.insert(source_id.clone());
            let service = self.service.clone();
            tasks.spawn(async move {
                let _result = service
                    .refresh_at(source_id.clone(), source.revision(), now_unix_ms)
                    .await;
                (source_id, ())
            });
        }
    }
}

fn select_new_sources(
    sources: Vec<SubscriptionSource>,
    active: &HashSet<String>,
    limit: usize,
) -> Vec<SubscriptionSource> {
    sources
        .into_iter()
        .filter(|source| !active.contains(source.id()))
        .take(limit)
        .collect()
}

pub(crate) fn remove_completed(
    completion: Option<Result<RefreshCompletion, tokio::task::JoinError>>,
    tasks: &JoinSet<RefreshCompletion>,
    active: &mut HashSet<String>,
) {
    if let Some(Ok((source_id, ()))) = completion {
        active.remove(&source_id);
    } else if tasks.is_empty() {
        active.clear();
    }
}
