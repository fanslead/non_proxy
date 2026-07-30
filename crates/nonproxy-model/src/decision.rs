use crate::{ModelError, OutboundId, PolicyId, RuleId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RouteAction {
    Direct,
    Proxy,
    Block,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureMode {
    Closed,
    Open,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DecisionSpec {
    action: RouteAction,
    outbound_id: Option<OutboundId>,
    failure_mode: FailureMode,
}

impl DecisionSpec {
    pub fn new(
        action: RouteAction,
        outbound_id: Option<OutboundId>,
        failure_mode: FailureMode,
    ) -> Result<Self, ModelError> {
        match (action, outbound_id.is_some()) {
            (RouteAction::Proxy, false) => return Err(ModelError::ProxyDecisionMissingOutbound),
            (RouteAction::Direct | RouteAction::Block, true) => {
                return Err(ModelError::NonProxyDecisionHasOutbound);
            }
            _ => {}
        }

        Ok(Self {
            action,
            outbound_id,
            failure_mode,
        })
    }

    pub fn direct() -> Self {
        Self {
            action: RouteAction::Direct,
            outbound_id: None,
            failure_mode: FailureMode::Closed,
        }
    }

    pub fn blocked() -> Self {
        Self {
            action: RouteAction::Block,
            outbound_id: None,
            failure_mode: FailureMode::Closed,
        }
    }

    #[must_use]
    pub const fn action(&self) -> RouteAction {
        self.action
    }

    #[must_use]
    pub fn outbound_id(&self) -> Option<&OutboundId> {
        self.outbound_id.as_ref()
    }

    #[must_use]
    pub const fn failure_mode(&self) -> FailureMode {
        self.failure_mode
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decision {
    result: DecisionSpec,
    matched_policy_id: Option<PolicyId>,
    matched_rule_id: Option<RuleId>,
    snapshot_version: u64,
    reason_code: &'static str,
}

impl Decision {
    #[must_use]
    pub fn matched(
        result: DecisionSpec,
        policy_id: PolicyId,
        rule_id: RuleId,
        snapshot_version: u64,
        reason_code: &'static str,
    ) -> Self {
        Self {
            result,
            matched_policy_id: Some(policy_id),
            matched_rule_id: Some(rule_id),
            snapshot_version,
            reason_code,
        }
    }

    #[must_use]
    pub fn defaulted(
        result: DecisionSpec,
        snapshot_version: u64,
        reason_code: &'static str,
    ) -> Self {
        Self {
            result,
            matched_policy_id: None,
            matched_rule_id: None,
            snapshot_version,
            reason_code,
        }
    }

    #[must_use]
    pub const fn result(&self) -> &DecisionSpec {
        &self.result
    }

    #[must_use]
    pub const fn matched_policy_id(&self) -> Option<&PolicyId> {
        self.matched_policy_id.as_ref()
    }

    #[must_use]
    pub const fn matched_rule_id(&self) -> Option<&RuleId> {
        self.matched_rule_id.as_ref()
    }

    #[must_use]
    pub const fn snapshot_version(&self) -> u64 {
        self.snapshot_version
    }

    #[must_use]
    pub const fn reason_code(&self) -> &'static str {
        self.reason_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_requires_an_outbound() {
        assert!(matches!(
            DecisionSpec::new(RouteAction::Proxy, None, FailureMode::Closed),
            Err(ModelError::ProxyDecisionMissingOutbound)
        ));
    }

    #[test]
    fn direct_rejects_an_outbound() {
        let outbound_result = OutboundId::new("local-socks");
        let Ok(outbound) = outbound_result else {
            panic!("测试出口标识创建失败: {outbound_result:?}");
        };

        assert!(matches!(
            DecisionSpec::new(RouteAction::Direct, Some(outbound), FailureMode::Open),
            Err(ModelError::NonProxyDecisionHasOutbound)
        ));
    }
}
