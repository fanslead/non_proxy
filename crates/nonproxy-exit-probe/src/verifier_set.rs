use std::collections::BTreeMap;

use crate::{ExitProbeError, ExitProbeReceipt, ExitProbeVerifier, ProbeNonce, VerifiedExitProbe};

pub const MAXIMUM_TRUSTED_EXIT_PROBE_KEYS: usize = 4;

#[derive(Clone, Debug)]
pub struct ExitProbeVerifierSet {
    verifiers: BTreeMap<String, ExitProbeVerifier>,
}

impl ExitProbeVerifierSet {
    pub fn new(
        verifiers: impl IntoIterator<Item = ExitProbeVerifier>,
    ) -> Result<Self, ExitProbeError> {
        let mut values = BTreeMap::new();
        for verifier in verifiers {
            if values.len() >= MAXIMUM_TRUSTED_EXIT_PROBE_KEYS
                || values
                    .insert(verifier.key_id().to_owned(), verifier)
                    .is_some()
            {
                return Err(ExitProbeError::KeyInvalid);
            }
        }
        if values.is_empty() {
            return Err(ExitProbeError::KeyInvalid);
        }
        Ok(Self { verifiers: values })
    }

    pub fn from_public_keys_base64<'key>(
        public_keys: impl IntoIterator<Item = &'key str>,
    ) -> Result<Self, ExitProbeError> {
        public_keys
            .into_iter()
            .map(ExitProbeVerifier::from_public_key_base64)
            .collect::<Result<Vec<_>, _>>()
            .and_then(Self::new)
    }

    pub fn verify(
        &self,
        expected_nonce: ProbeNonce,
        receipt: ExitProbeReceipt,
        now_unix_ms: u64,
    ) -> Result<VerifiedExitProbe, ExitProbeError> {
        let verifier = self
            .verifiers
            .get(&receipt.key_id)
            .ok_or(ExitProbeError::ResponseInvalid)?;
        verifier.verify(expected_nonce, receipt, now_unix_ms)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.verifiers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.verifiers.is_empty()
    }
}

impl From<ExitProbeVerifier> for ExitProbeVerifierSet {
    fn from(verifier: ExitProbeVerifier) -> Self {
        let mut verifiers = BTreeMap::new();
        verifiers.insert(verifier.key_id().to_owned(), verifier);
        Self { verifiers }
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    use super::{ExitProbeVerifierSet, MAXIMUM_TRUSTED_EXIT_PROBE_KEYS};
    use crate::{ExitProbeError, ExitProbeSigner, ExitProbeVerifier, ProbeNonce};

    #[test]
    fn rotation_window_accepts_old_and_new_signers_but_rejects_unknown_key() {
        let old = signer(1);
        let new = signer(2);
        let unknown = signer(3);
        let verifiers = ExitProbeVerifierSet::from_public_keys_base64([
            old.public_key_base64().as_str(),
            new.public_key_base64().as_str(),
        ]);
        let Ok(verifiers) = verifiers else {
            panic!("轮换验签集合创建失败: {verifiers:?}");
        };
        let nonce = nonce(9);

        assert!(
            old.sign(nonce, Ipv4Addr::new(8, 8, 8, 8).into(), 1_000)
                .and_then(|receipt| verifiers.verify(nonce, receipt, 1_000))
                .is_ok()
        );
        assert!(
            new.sign(nonce, Ipv4Addr::new(8, 8, 4, 4).into(), 1_000)
                .and_then(|receipt| verifiers.verify(nonce, receipt, 1_000))
                .is_ok()
        );
        assert!(matches!(
            unknown
                .sign(nonce, Ipv4Addr::new(1, 1, 1, 1).into(), 1_000)
                .and_then(|receipt| verifiers.verify(nonce, receipt, 1_000)),
            Err(ExitProbeError::ResponseInvalid)
        ));
    }

    #[test]
    fn trusted_key_collection_rejects_empty_duplicates_and_excess() {
        assert!(ExitProbeVerifierSet::new([]).is_err());
        let duplicate = ExitProbeVerifier::from_public_key_base64(&signer(1).public_key_base64())
            .unwrap_or_else(|error| panic!("测试验签器创建失败: {error}"));
        assert!(ExitProbeVerifierSet::new([duplicate.clone(), duplicate]).is_err());
        let excess = (0..=MAXIMUM_TRUSTED_EXIT_PROBE_KEYS)
            .map(|index| {
                ExitProbeVerifier::from_public_key_base64(
                    &signer(u8::try_from(index + 1).unwrap_or(u8::MAX)).public_key_base64(),
                )
                .unwrap_or_else(|error| panic!("测试验签器创建失败: {error}"))
            })
            .collect::<Vec<_>>();
        assert!(ExitProbeVerifierSet::new(excess).is_err());
    }

    fn signer(seed: u8) -> ExitProbeSigner {
        ExitProbeSigner::from_secret_bytes(&[seed; 32])
            .unwrap_or_else(|error| panic!("测试签名器创建失败: {error}"))
    }

    fn nonce(seed: u8) -> ProbeNonce {
        ProbeNonce::from_base64(&URL_SAFE_NO_PAD.encode([seed; 32]))
            .unwrap_or_else(|error| panic!("测试 nonce 创建失败: {error}"))
    }
}
