use crate::{ModelError, NetworkProfileId};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NetworkFingerprintKind {
    WifiSsidSha256,
    DefaultGatewaySha256,
    InterfaceClass,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NetworkFingerprint {
    kind: NetworkFingerprintKind,
    value: String,
}

impl NetworkFingerprint {
    pub fn new(kind: NetworkFingerprintKind, value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        let valid = match kind {
            NetworkFingerprintKind::WifiSsidSha256
            | NetworkFingerprintKind::DefaultGatewaySha256 => {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }
            NetworkFingerprintKind::InterfaceClass => {
                matches!(value.as_str(), "wifi" | "ethernet" | "cellular" | "other")
            }
        };
        if !valid {
            return Err(ModelError::InvalidNetworkFingerprint);
        }
        Ok(Self { kind, value })
    }

    #[must_use]
    pub const fn kind(&self) -> NetworkFingerprintKind {
        self.kind
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkProfileBinding {
    id: NetworkProfileId,
    fingerprint: NetworkFingerprint,
}

impl NetworkProfileBinding {
    #[must_use]
    pub const fn new(id: NetworkProfileId, fingerprint: NetworkFingerprint) -> Self {
        Self { id, fingerprint }
    }

    #[must_use]
    pub const fn id(&self) -> &NetworkProfileId {
        &self.id
    }

    #[must_use]
    pub const fn fingerprint(&self) -> &NetworkFingerprint {
        &self.fingerprint
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkProfileReference {
    id: NetworkProfileId,
    display_name: String,
    fingerprint: NetworkFingerprint,
    revision: u64,
}

impl NetworkProfileReference {
    pub fn new(
        id: NetworkProfileId,
        display_name: impl Into<String>,
        fingerprint: NetworkFingerprint,
        revision: u64,
    ) -> Result<Self, ModelError> {
        let display_name = display_name.into();
        if display_name.is_empty()
            || display_name.len() > 128
            || display_name.chars().any(char::is_control)
        {
            return Err(ModelError::InvalidNetworkProfileDisplayName);
        }
        if revision == 0 {
            return Err(ModelError::InvalidNetworkProfileRevision);
        }
        Ok(Self {
            id,
            display_name,
            fingerprint,
            revision,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &NetworkProfileId {
        &self.id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn fingerprint(&self) -> &NetworkFingerprint {
        &self.fingerprint
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn binding(&self) -> NetworkProfileBinding {
        NetworkProfileBinding::new(self.id.clone(), self.fingerprint.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{NetworkFingerprint, NetworkFingerprintKind};

    #[test]
    fn raw_wifi_name_is_not_a_valid_privacy_fingerprint() {
        assert!(
            NetworkFingerprint::new(NetworkFingerprintKind::WifiSsidSha256, "Office WiFi").is_err()
        );
    }

    #[test]
    fn interface_class_is_bounded_to_portable_values() {
        assert!(NetworkFingerprint::new(NetworkFingerprintKind::InterfaceClass, "wifi").is_ok());
        assert!(NetworkFingerprint::new(NetworkFingerprintKind::InterfaceClass, "bridge").is_err());
    }
}
