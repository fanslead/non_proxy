use sha2::{Digest, Sha256};

use crate::{
    AdapterCapability, AdapterClient, AdapterContractError, AdapterVersion, NormalizedPolicy,
};

const MAXIMUM_RENDERED_RULE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedRules {
    client: AdapterClient,
    format: &'static str,
    bytes: Vec<u8>,
    sha256: [u8; 32],
    rule_count: usize,
}

impl RenderedRules {
    pub fn new(
        client: AdapterClient,
        format: &'static str,
        bytes: Vec<u8>,
        rule_count: usize,
    ) -> Result<Self, AdapterContractError> {
        if bytes.len() > MAXIMUM_RENDERED_RULE_BYTES {
            return Err(AdapterContractError::RenderedRulesTooLarge);
        }
        let sha256 = Sha256::digest(&bytes).into();
        Ok(Self {
            client,
            format,
            bytes,
            sha256,
            rule_count,
        })
    }

    #[must_use]
    pub const fn client(&self) -> AdapterClient {
        self.client
    }

    #[must_use]
    pub const fn format(&self) -> &'static str {
        self.format
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }

    #[must_use]
    pub const fn rule_count(&self) -> usize {
        self.rule_count
    }
}

pub trait AdapterRenderer: Send + Sync {
    fn client(&self) -> AdapterClient;

    fn capabilities(&self, version: AdapterVersion) -> Vec<AdapterCapability>;

    fn render(
        &self,
        version: AdapterVersion,
        policy: &NormalizedPolicy,
    ) -> Result<RenderedRules, AdapterContractError>;
}
