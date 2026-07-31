use std::{sync::Arc, time::Duration};

use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_windows_wfp::{WfpConfig, WfpDriver};
use tokio::{
    sync::watch,
    time::{MissedTickBehavior, interval},
};

use crate::GatewayError;

use super::{data_plane_error, policy_cache::WindowsPolicyCache};

const RECONCILE_INTERVAL: Duration = Duration::from_millis(100);

pub struct WfpActivation {
    driver: Arc<WfpDriver>,
    policies: WindowsPolicyCache,
    generation: u64,
    process_id: u64,
    ports: WfpRedirectPorts,
    dns_ready: watch::Receiver<bool>,
    enabled: bool,
}

#[derive(Clone, Copy)]
pub(super) struct WfpRedirectPorts {
    pub tcp_ipv4: u16,
    pub tcp_ipv6: u16,
    pub dns_ipv4: u16,
    pub dns_ipv6: u16,
}

impl WfpActivation {
    pub fn new(
        driver: Arc<WfpDriver>,
        policies: WindowsPolicyCache,
        generation: u64,
        process_id: u64,
        ports: WfpRedirectPorts,
        dns_ready: watch::Receiver<bool>,
    ) -> Self {
        Self {
            driver,
            policies,
            generation,
            process_id,
            ports,
            dns_ready,
            enabled: false,
        }
    }

    pub async fn serve(mut self, mut shutdown: watch::Receiver<bool>) -> Result<(), GatewayError> {
        let mut ticker = interval(RECONCILE_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                _ = ticker.tick() => self.reconcile().await?,
                changed = self.dns_ready.changed() => {
                    let _changed = changed;
                    self.reconcile().await?;
                }
            }
        }
        self.disable_all()?;
        Ok(())
    }

    async fn reconcile(&mut self) -> Result<(), GatewayError> {
        let active = self.policies.current().await;
        let dns_ready = *self.dns_ready.borrow();
        match (active, dns_ready) {
            (Some(snapshot), true) => {
                if !self.enabled {
                    self.generation = next_generation(self.generation)?;
                    self.driver
                        .apply(&WfpConfig::enabled(
                            self.generation,
                            self.process_id,
                            self.ports.tcp_ipv4,
                            self.ports.tcp_ipv6,
                            self.ports.dns_ipv4,
                            self.ports.dns_ipv6,
                        ))
                        .map_err(data_plane_error)?;
                    self.enabled = true;
                }
                self.policies
                    .report_health(RuntimeState::Ready, snapshot.metadata().snapshot_version())?;
            }
            (snapshot, false) => {
                self.disable_tcp()?;
                let version = snapshot
                    .as_ref()
                    .map_or(0, |value| value.metadata().snapshot_version());
                self.policies
                    .report_health(RuntimeState::Degraded, version)?;
            }
            (None, true) => {
                self.disable_tcp()?;
                self.policies.report_health(RuntimeState::Starting, 0)?;
            }
        }
        Ok(())
    }

    fn disable_tcp(&mut self) -> Result<(), GatewayError> {
        if !self.enabled {
            return Ok(());
        }
        self.generation = next_generation(self.generation)?;
        self.driver
            .apply(&WfpConfig::dns_only(
                self.generation,
                self.process_id,
                self.ports.dns_ipv4,
                self.ports.dns_ipv6,
            ))
            .map_err(data_plane_error)?;
        self.enabled = false;
        Ok(())
    }

    fn disable_all(&mut self) -> Result<(), GatewayError> {
        self.generation = next_generation(self.generation)?;
        self.driver
            .apply(&WfpConfig::disabled(self.generation))
            .map_err(data_plane_error)?;
        self.enabled = false;
        Ok(())
    }
}

fn next_generation(current: u64) -> Result<u64, GatewayError> {
    current
        .checked_add(1)
        .ok_or_else(|| GatewayError::WindowsDataPlane("WFP 配置代次耗尽".to_owned()))
}
