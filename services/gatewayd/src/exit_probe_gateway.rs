use nonproxy_exit_probe::VerifiedExitProbe;
use nonproxy_storage::{ExitProbeInput, ExitProbeRecord, ExitProbeRoute};

use crate::{Gateway, GatewayError, clock::unix_time_ms};

impl Gateway {
    pub(crate) async fn save_exit_probe(
        &self,
        route: ExitProbeRoute,
        verified: &VerifiedExitProbe,
    ) -> Result<i64, GatewayError> {
        let input = ExitProbeInput::new(
            verified.probe_id(),
            route,
            verified.observed_ip(),
            verified.observed_at_unix_ms(),
            verified.key_id(),
            unix_time_ms()?,
        )?;
        self.database
            .run(move |database| Ok(database.exit_probes().save(&input)?))
            .await
    }

    pub(crate) async fn list_exit_probes(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<ExitProbeRecord>, u64), GatewayError> {
        self.database
            .run(move |database| Ok(database.exit_probes().list_recent(limit, offset)?))
            .await
    }
}
