use std::{net::IpAddr, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::ExitProbeError;

const RECEIPT_SCHEMA_VERSION: u32 = 1;
const NONCE_BYTES: usize = 32;
const PUBLIC_KEY_BYTES: usize = 32;
const SIGNATURE_BYTES: usize = 64;
const MAXIMUM_RECEIPT_AGE: Duration = Duration::from_secs(120);
const MAXIMUM_FUTURE_SKEW: Duration = Duration::from_secs(300);
const CANONICAL_PREFIX: &[u8] = b"nonproxy-exit-probe-v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeNonce([u8; NONCE_BYTES]);

impl ProbeNonce {
    pub fn generate() -> Result<Self, ExitProbeError> {
        let mut value = [0_u8; NONCE_BYTES];
        getrandom::fill(&mut value).map_err(|_| ExitProbeError::Random)?;
        Ok(Self(value))
    }

    pub fn from_base64(value: &str) -> Result<Self, ExitProbeError> {
        decode_array(value).map(Self)
    }

    #[must_use]
    pub fn to_base64(self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; NONCE_BYTES] {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExitProbeReceipt {
    pub schema_version: u32,
    pub nonce: String,
    pub observed_ip: String,
    pub observed_at_unix_ms: u64,
    pub key_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedExitProbe {
    probe_id: String,
    observed_ip: IpAddr,
    observed_at_unix_ms: u64,
    key_id: String,
}

impl VerifiedExitProbe {
    #[must_use]
    pub fn probe_id(&self) -> &str {
        &self.probe_id
    }

    #[must_use]
    pub const fn observed_ip(&self) -> IpAddr {
        self.observed_ip
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> u64 {
        self.observed_at_unix_ms
    }

    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
}

pub struct ExitProbeSigner {
    signing_key: SigningKey,
    key_id: String,
}

impl ExitProbeSigner {
    pub fn from_secret_bytes(secret: &[u8]) -> Result<Self, ExitProbeError> {
        let bytes = Zeroizing::new(secret.try_into().map_err(|_| ExitProbeError::KeyInvalid)?);
        let signing_key = SigningKey::from_bytes(&bytes);
        let key_id = key_id(&signing_key.verifying_key());
        Ok(Self {
            signing_key,
            key_id,
        })
    }

    #[must_use]
    pub fn public_key_base64(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.signing_key.verifying_key().to_bytes())
    }

    pub fn sign(
        &self,
        nonce: ProbeNonce,
        observed_ip: IpAddr,
        observed_at_unix_ms: u64,
    ) -> Result<ExitProbeReceipt, ExitProbeError> {
        validate_public_ip(observed_ip)?;
        if observed_at_unix_ms == 0 {
            return Err(ExitProbeError::TimestampInvalid);
        }
        let canonical = canonical(
            nonce.as_bytes(),
            observed_ip,
            observed_at_unix_ms,
            &self.key_id,
        );
        let signature = self.signing_key.sign(&canonical);
        Ok(ExitProbeReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            nonce: nonce.to_base64(),
            observed_ip: observed_ip.to_string(),
            observed_at_unix_ms,
            key_id: self.key_id.clone(),
            signature: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ExitProbeVerifier {
    verifying_key: VerifyingKey,
    key_id: String,
}

impl ExitProbeVerifier {
    pub fn from_public_key_base64(value: &str) -> Result<Self, ExitProbeError> {
        let bytes: [u8; PUBLIC_KEY_BYTES] = decode_array(value)?;
        let verifying_key =
            VerifyingKey::from_bytes(&bytes).map_err(|_| ExitProbeError::KeyInvalid)?;
        let key_id = key_id(&verifying_key);
        Ok(Self {
            verifying_key,
            key_id,
        })
    }

    pub fn verify(
        &self,
        expected_nonce: ProbeNonce,
        receipt: ExitProbeReceipt,
        now_unix_ms: u64,
    ) -> Result<VerifiedExitProbe, ExitProbeError> {
        if receipt.schema_version != RECEIPT_SCHEMA_VERSION || receipt.key_id != self.key_id {
            return Err(ExitProbeError::ResponseInvalid);
        }
        let nonce = ProbeNonce::from_base64(&receipt.nonce)?;
        if nonce != expected_nonce {
            return Err(ExitProbeError::NonceMismatch);
        }
        let observed_ip = receipt
            .observed_ip
            .parse::<IpAddr>()
            .map_err(|_| ExitProbeError::AddressInvalid)?;
        validate_public_ip(observed_ip)?;
        validate_timestamp(receipt.observed_at_unix_ms, now_unix_ms)?;
        let signature_bytes: [u8; SIGNATURE_BYTES] = decode_array(&receipt.signature)?;
        let signature = Signature::from_bytes(&signature_bytes);
        let canonical = canonical(
            nonce.as_bytes(),
            observed_ip,
            receipt.observed_at_unix_ms,
            &receipt.key_id,
        );
        self.verifying_key
            .verify(&canonical, &signature)
            .map_err(|_| ExitProbeError::SignatureInvalid)?;
        let mut digest = Sha256::new();
        digest.update(&canonical);
        digest.update(signature_bytes);
        Ok(VerifiedExitProbe {
            probe_id: URL_SAFE_NO_PAD.encode(digest.finalize()),
            observed_ip,
            observed_at_unix_ms: receipt.observed_at_unix_ms,
            key_id: receipt.key_id,
        })
    }
}

fn validate_public_ip(value: IpAddr) -> Result<(), ExitProbeError> {
    let is_public = match value {
        IpAddr::V4(address) => is_public_ipv4(address.octets()),
        IpAddr::V6(address) => {
            let segments = address.segments();
            segments[0] & 0xe000 == 0x2000 && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    };
    if is_public {
        Ok(())
    } else {
        Err(ExitProbeError::AddressInvalid)
    }
}

fn is_public_ipv4(value: [u8; 4]) -> bool {
    !matches!(
        value,
        [0, ..]
            | [10, ..]
            | [100, 64..=127, ..]
            | [127, ..]
            | [169, 254, ..]
            | [172, 16..=31, ..]
            | [192, 0, 0, ..]
            | [192, 0, 2, ..]
            | [192, 88, 99, ..]
            | [192, 168, ..]
            | [198, 18..=19, ..]
            | [198, 51, 100, ..]
            | [203, 0, 113, ..]
            | [224..=255, ..]
    )
}

fn validate_timestamp(observed_at: u64, now: u64) -> Result<(), ExitProbeError> {
    let oldest = now.saturating_sub(duration_ms(MAXIMUM_RECEIPT_AGE));
    let newest = now
        .checked_add(duration_ms(MAXIMUM_FUTURE_SKEW))
        .ok_or(ExitProbeError::TimestampInvalid)?;
    if observed_at == 0 || observed_at < oldest || observed_at > newest {
        return Err(ExitProbeError::TimestampInvalid);
    }
    Ok(())
}

fn canonical(nonce: &[u8; NONCE_BYTES], ip: IpAddr, observed_at: u64, key_id: &str) -> Vec<u8> {
    let ip = ip.to_string();
    let mut output =
        Vec::with_capacity(CANONICAL_PREFIX.len() + NONCE_BYTES + ip.len() + key_id.len() + 20);
    output.extend_from_slice(CANONICAL_PREFIX);
    append_field(&mut output, nonce);
    append_field(&mut output, ip.as_bytes());
    output.extend_from_slice(&observed_at.to_be_bytes());
    append_field(&mut output, key_id.as_bytes());
    output
}

fn append_field(output: &mut Vec<u8>, value: &[u8]) {
    let length = u32::try_from(value.len()).unwrap_or(u32::MAX);
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
}

fn key_id(key: &VerifyingKey) -> String {
    let digest = Sha256::digest(key.to_bytes());
    URL_SAFE_NO_PAD.encode(&digest[..16])
}

fn decode_array<const LENGTH: usize>(value: &str) -> Result<[u8; LENGTH], ExitProbeError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ExitProbeError::ResponseInvalid)?;
    bytes
        .try_into()
        .map_err(|_| ExitProbeError::ResponseInvalid)
}

fn duration_ms(value: Duration) -> u64 {
    value.as_secs().saturating_mul(1_000) + u64::from(value.subsec_millis())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::{ExitProbeSigner, ExitProbeVerifier, ProbeNonce};
    use crate::ExitProbeError;

    const NOW: u64 = 2_000_000;

    #[test]
    fn signed_receipt_round_trip_binds_nonce_ip_time_and_key() {
        let signer = signer(7);
        let verifier = ExitProbeVerifier::from_public_key_base64(&signer.public_key_base64());
        let Ok(verifier) = verifier else {
            panic!("测试验签器创建失败: {verifier:?}");
        };
        let nonce = nonce(9);
        let receipt = signer.sign(nonce, public_ip(), NOW);
        let Ok(receipt) = receipt else {
            panic!("测试回执签名失败: {receipt:?}");
        };

        let verified = verifier.verify(nonce, receipt, NOW + 1);
        let Ok(verified) = verified else {
            panic!("测试回执验签失败: {verified:?}");
        };

        assert_eq!(verified.observed_ip(), public_ip());
        assert_eq!(verified.observed_at_unix_ms(), NOW);
        assert_eq!(verified.probe_id().len(), 43);
        assert_eq!(verified.key_id().len(), 22);
    }

    #[test]
    fn rejects_tampering_replay_stale_time_and_private_addresses() {
        let signer = signer(7);
        let verifier = ExitProbeVerifier::from_public_key_base64(&signer.public_key_base64());
        let Ok(verifier) = verifier else {
            panic!("测试验签器创建失败: {verifier:?}");
        };
        let expected = nonce(9);
        let receipt = signer.sign(expected, public_ip(), NOW);
        let Ok(receipt) = receipt else {
            panic!("测试回执签名失败: {receipt:?}");
        };
        let mut tampered = receipt.clone();
        tampered.observed_ip = "8.8.4.4".to_owned();

        assert!(matches!(
            verifier.verify(expected, tampered, NOW),
            Err(ExitProbeError::SignatureInvalid)
        ));
        assert!(matches!(
            verifier.verify(nonce(10), receipt.clone(), NOW),
            Err(ExitProbeError::NonceMismatch)
        ));
        assert!(matches!(
            verifier.verify(expected, receipt, NOW + 120_001),
            Err(ExitProbeError::TimestampInvalid)
        ));
        assert!(matches!(
            signer.sign(expected, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), NOW),
            Err(ExitProbeError::AddressInvalid)
        ));
    }

    fn signer(seed: u8) -> ExitProbeSigner {
        match ExitProbeSigner::from_secret_bytes(&[seed; 32]) {
            Ok(value) => value,
            Err(error) => panic!("测试签名器创建失败: {error}"),
        }
    }

    fn nonce(seed: u8) -> ProbeNonce {
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            [seed; 32],
        );
        match ProbeNonce::from_base64(&encoded) {
            Ok(value) => value,
            Err(error) => panic!("测试 nonce 创建失败: {error}"),
        }
    }

    fn public_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))
    }
}
