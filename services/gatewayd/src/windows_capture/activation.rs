use std::time::Duration;

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
    driver: WfpDriver,
    policies: WindowsPolicyCache,
    generation: u64,
    process_id: u64,
    ipv4_port: u16,
    ipv6_port: u16,
    enabled: bool,
}

impl WfpActivation {
    pub fn new(
        driver: WfpDriver,
        policies: WindowsPolicyCache,
        generation: u64,
        process_id: u64,
        ipv4_port: u16,
        ipv6_port: u16,
    ) -> Self {
        Self {
            driver,
            policies,
            generation,
            process_id,
            ipv4_port,
            ipv6_port,
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
            }
        }
        self.disable()?;
        Ok(())
    }

    async fn reconcile(&mut self) -> Result<(), GatewayError> {
        let active = self.policies.current().await;
        match active {
            Some(snapshot) => {
                if !self.enabled {
                    self.generation = next_generation(self.generation)?;
                    self.driver
                        .apply(&WfpConfig::enabled(
                            self.generation,
                            self.process_id,
                            self.ipv4_port,
                            self.ipv6_port,
                        ))
                        .map_err(data_plane_error)?;
                    self.enabled = true;
                }
                self.policies
                    .report_health(RuntimeState::Ready, snapshot.metadata().snapshot_version())?;
            }
            None => {
                self.disable()?;
                self.policies.report_health(RuntimeState::Starting, 0)?;
            }
        }
        Ok(())
    }

    fn disable(&mut self) -> Result<(), GatewayError> {
        if !self.enabled {
            return Ok(());
        }
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
