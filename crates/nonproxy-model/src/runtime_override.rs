use crate::{DecisionSpec, FailureMode, ModelError, OutboundId, RouteAction};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeOverrideMode {
    Paused,
    Direct,
    Proxy,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeRoutingOverride {
    mode: RuntimeOverrideMode,
    outbound_id: Option<OutboundId>,
    expires_at_unix_ms: u64,
}

impl RuntimeRoutingOverride {
    pub fn new(
        mode: RuntimeOverrideMode,
        outbound_id: Option<OutboundId>,
        expires_at_unix_ms: u64,
    ) -> Result<Self, ModelError> {
        match (mode, outbound_id.is_some()) {
            (RuntimeOverrideMode::Proxy, false) => {
                return Err(ModelError::RuntimeOverrideProxyMissingOutbound);
            }
            (RuntimeOverrideMode::Paused | RuntimeOverrideMode::Direct, true) => {
                return Err(ModelError::RuntimeOverrideNonProxyHasOutbound);
            }
            _ => {}
        }
        if expires_at_unix_ms == 0 {
            return Err(ModelError::RuntimeOverrideExpiryInvalid);
        }
        Ok(Self {
            mode,
            outbound_id,
            expires_at_unix_ms,
        })
    }

    #[must_use]
    pub const fn mode(&self) -> RuntimeOverrideMode {
        self.mode
    }

    #[must_use]
    pub fn outbound_id(&self) -> Option<&OutboundId> {
        self.outbound_id.as_ref()
    }

    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }

    #[must_use]
    pub const fn is_active_at(&self, unix_time_ms: u64) -> bool {
        unix_time_ms < self.expires_at_unix_ms
    }

    pub fn decision(&self) -> Result<Option<DecisionSpec>, ModelError> {
        match self.mode {
            RuntimeOverrideMode::Paused => Ok(None),
            RuntimeOverrideMode::Direct => Ok(Some(DecisionSpec::direct())),
            RuntimeOverrideMode::Proxy => DecisionSpec::new(
                RouteAction::Proxy,
                self.outbound_id.clone(),
                FailureMode::Closed,
            )
            .map(Some),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_and_outbound_shape_is_strict() {
        let outbound = OutboundId::new("default-proxy");
        let Ok(outbound) = outbound else {
            panic!("测试出口创建失败: {outbound:?}");
        };
        assert!(matches!(
            RuntimeRoutingOverride::new(RuntimeOverrideMode::Proxy, None, 2_000),
            Err(ModelError::RuntimeOverrideProxyMissingOutbound)
        ));
        assert!(matches!(
            RuntimeRoutingOverride::new(RuntimeOverrideMode::Direct, Some(outbound), 2_000),
            Err(ModelError::RuntimeOverrideNonProxyHasOutbound)
        ));
    }

    #[test]
    fn expiration_boundary_is_exclusive() {
        let value = RuntimeRoutingOverride::new(RuntimeOverrideMode::Paused, None, 2_000);
        let Ok(value) = value else {
            panic!("暂停覆盖创建失败: {value:?}");
        };
        assert!(value.is_active_at(1_999));
        assert!(!value.is_active_at(2_000));
    }
}
