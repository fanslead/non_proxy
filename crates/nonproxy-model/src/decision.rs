use crate::{ModelError, OutboundGroupId, OutboundId, PolicyId, RuleId};

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
pub enum ProxyTarget {
    Outbound(OutboundId),
    Group(OutboundGroupId),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DecisionSpec {
    action: RouteAction,
    proxy_target: Option<ProxyTarget>,
    failure_mode: FailureMode,
}

impl DecisionSpec {
    pub fn new(
        action: RouteAction,
        outbound_id: Option<OutboundId>,
        failure_mode: FailureMode,
    ) -> Result<Self, ModelError> {
        Self::new_with_target(action, outbound_id.map(ProxyTarget::Outbound), failure_mode)
    }

    pub fn new_with_target(
        action: RouteAction,
        proxy_target: Option<ProxyTarget>,
        failure_mode: FailureMode,
    ) -> Result<Self, ModelError> {
        match (action, proxy_target.is_some()) {
            (RouteAction::Proxy, false) => return Err(ModelError::ProxyDecisionMissingOutbound),
            (RouteAction::Direct | RouteAction::Block, true) => {
                return Err(ModelError::NonProxyDecisionHasOutbound);
            }
            _ => {}
        }

        Ok(Self {
            action,
            proxy_target,
            failure_mode,
        })
    }

    pub fn proxy_group(
        group_id: OutboundGroupId,
        failure_mode: FailureMode,
    ) -> Result<Self, ModelError> {
        Self::new_with_target(
            RouteAction::Proxy,
            Some(ProxyTarget::Group(group_id)),
            failure_mode,
        )
    }

    pub fn direct() -> Self {
        Self {
            action: RouteAction::Direct,
            proxy_target: None,
            failure_mode: FailureMode::Closed,
        }
    }

    pub fn blocked() -> Self {
        Self {
            action: RouteAction::Block,
            proxy_target: None,
            failure_mode: FailureMode::Closed,
        }
    }

    #[must_use]
    pub const fn action(&self) -> RouteAction {
        self.action
    }

    #[must_use]
    pub fn outbound_id(&self) -> Option<&OutboundId> {
        match self.proxy_target.as_ref() {
            Some(ProxyTarget::Outbound(value)) => Some(value),
            Some(ProxyTarget::Group(_)) | None => None,
        }
    }

    #[must_use]
    pub fn outbound_group_id(&self) -> Option<&OutboundGroupId> {
        match self.proxy_target.as_ref() {
            Some(ProxyTarget::Group(value)) => Some(value),
            Some(ProxyTarget::Outbound(_)) | None => None,
        }
    }

    #[must_use]
    pub const fn proxy_target(&self) -> Option<&ProxyTarget> {
        self.proxy_target.as_ref()
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
    fn proxy_requires_a_target() {
        assert!(matches!(
            DecisionSpec::new(RouteAction::Proxy, None, FailureMode::Closed),
            Err(ModelError::ProxyDecisionMissingOutbound)
        ));
    }

    #[test]
    fn direct_rejects_a_proxy_target() {
        let outbound_result = OutboundId::new("local-socks");
        let Ok(outbound) = outbound_result else {
            panic!("测试出口标识创建失败: {outbound_result:?}");
        };

        assert!(matches!(
            DecisionSpec::new(RouteAction::Direct, Some(outbound), FailureMode::Open),
            Err(ModelError::NonProxyDecisionHasOutbound)
        ));
    }

    #[test]
    fn proxy_group_is_explicit_and_never_aliases_an_outbound() {
        let group = OutboundGroupId::new("office-failover");
        let Ok(group) = group else {
            panic!("测试出口组标识创建失败: {group:?}");
        };
        let decision = DecisionSpec::proxy_group(group.clone(), FailureMode::Open);
        let Ok(decision) = decision else {
            panic!("测试出口组决策创建失败: {decision:?}");
        };

        assert_eq!(decision.outbound_group_id(), Some(&group));
        assert!(decision.outbound_id().is_none());
        assert!(matches!(
            decision.proxy_target(),
            Some(ProxyTarget::Group(value)) if value == &group
        ));
    }
}
