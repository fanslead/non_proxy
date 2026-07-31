use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nonproxy_exit_probe::{
    ExitProbeClient, ExitProbeEndpoint, ExitProbeVerifierSet, ProbeNonce, VerifiedExitProbe,
};

use crate::AdminError;

const VERIFICATION_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run(endpoint: &str, public_keys: &[String]) -> Result<VerifiedExitProbe, AdminError> {
    let endpoint = ExitProbeEndpoint::parse(endpoint).map_err(|_| AdminError::Verification)?;
    let verifiers =
        ExitProbeVerifierSet::from_public_keys_base64(public_keys.iter().map(String::as_str))
            .map_err(|_| AdminError::Verification)?;
    let stream = tokio::time::timeout(
        VERIFICATION_TIMEOUT,
        tokio::net::TcpStream::connect((endpoint.host(), endpoint.port())),
    )
    .await
    .map_err(|_| AdminError::Verification)?
    .map_err(|_| AdminError::Verification)?;
    let client = ExitProbeClient::new(endpoint, verifiers).map_err(|_| AdminError::Verification)?;
    let nonce = ProbeNonce::generate().map_err(|_| AdminError::Random)?;
    client
        .probe(stream, nonce, unix_time_ms()?, VERIFICATION_TIMEOUT)
        .await
        .map_err(|_| AdminError::Verification)
}

fn unix_time_ms() -> Result<u64, AdminError> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AdminError::Clock)?
        .as_millis();
    u64::try_from(value).map_err(|_| AdminError::Clock)
}
