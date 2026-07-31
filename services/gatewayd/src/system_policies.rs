use nonproxy_model::{
    AppMatcher, DecisionSpec, Platform, Policy, PolicyId, PolicyMatch, PolicyMetadata,
    PolicyOrigin, PolicySourceKind,
};

use crate::GatewayError;

pub(crate) const MACOS_GATEWAY_SIGNING_IDENTIFIER: &str = "com.nonproxy.gatewayd";
const MACOS_GATEWAY_POLICY_ID: &str = "system-macos-gateway-direct";
const SYSTEM_POLICY_REVISION: u64 = 1;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SystemPolicyConfig {
    macos_gateway_signer_id: Option<String>,
}

impl SystemPolicyConfig {
    pub(crate) fn new(macos_gateway_signer_id: Option<String>) -> Result<Self, GatewayError> {
        if let Some(signer_id) = macos_gateway_signer_id.as_deref() {
            AppMatcher::new(Platform::MacOs, MACOS_GATEWAY_SIGNING_IDENTIFIER)?
                .with_signer_id(signer_id)?;
        }
        Ok(Self {
            macos_gateway_signer_id,
        })
    }

    #[must_use]
    pub(crate) fn macos_gateway_signer_id(&self) -> Option<&str> {
        self.macos_gateway_signer_id.as_deref()
    }
}

pub(crate) fn with_required(
    policies: &[Policy],
    config: &SystemPolicyConfig,
) -> Result<Vec<Policy>, GatewayError> {
    let mut result = Vec::with_capacity(policies.len() + 1);
    result.extend(
        policies
            .iter()
            .filter(|policy| !is_reserved_id(policy.id()))
            .cloned(),
    );
    result.push(macos_gateway_direct(config)?);
    Ok(result)
}

pub(crate) fn contains_required(
    policies: &[Policy],
    config: &SystemPolicyConfig,
) -> Result<bool, GatewayError> {
    let required = macos_gateway_direct(config)?;
    Ok(policies
        .iter()
        .filter(|policy| is_reserved_id(policy.id()))
        .eq([&required]))
}

pub(crate) fn validate_user_mutation(policy: &Policy) -> Result<(), GatewayError> {
    let editable_source = matches!(
        policy.source_kind(),
        PolicySourceKind::AppDestination
            | PolicySourceKind::App
            | PolicySourceKind::Site
            | PolicySourceKind::Network
            | PolicySourceKind::Cidr
    );
    if !editable_source || policy.origin() != PolicyOrigin::User || is_reserved_id(policy.id()) {
        return Err(GatewayError::InvalidContract(
            "客户端不能创建或覆盖受保护策略",
        ));
    }
    Ok(())
}

fn macos_gateway_direct(config: &SystemPolicyConfig) -> Result<Policy, GatewayError> {
    let mut app = AppMatcher::new(Platform::MacOs, MACOS_GATEWAY_SIGNING_IDENTIFIER)?;
    if let Some(signer_id) = config.macos_gateway_signer_id() {
        app = app.with_signer_id(signer_id)?;
    }
    let matcher = PolicyMatch::new(Some(app), None, None, None, Vec::new(), Vec::new())?;
    Ok(Policy::new(
        PolicyId::new(MACOS_GATEWAY_POLICY_ID)?,
        "NonProxy 后台服务防回环",
        matcher,
        DecisionSpec::direct(),
        PolicyMetadata::new(
            PolicySourceKind::System,
            i32::MAX,
            PolicyOrigin::System,
            SYSTEM_POLICY_REVISION,
        ),
    )?)
}

fn is_reserved_id(policy_id: &PolicyId) -> bool {
    policy_id.as_str() == MACOS_GATEWAY_POLICY_ID
}

pub(crate) fn is_managed_system_policy(policy: &Policy) -> bool {
    is_reserved_id(policy.id())
}

#[cfg(test)]
mod tests {
    use nonproxy_model::{
        AppIdentity, ConnectionContext, Destination, Platform, PolicyId, RouteAction, Transport,
    };
    use nonproxy_policy::OutboundCapabilities;
    use nonproxy_policy::PolicyEngine;
    use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};

    use super::{
        MACOS_GATEWAY_POLICY_ID, MACOS_GATEWAY_SIGNING_IDENTIFIER, SystemPolicyConfig,
        contains_required, with_required,
    };

    #[test]
    fn required_rule_forces_only_the_signed_macos_gateway_direct() {
        let config = signed_config();
        let policies = match with_required(&[], &config) {
            Ok(value) => value,
            Err(error) => panic!("系统防回环策略创建失败: {error}"),
        };
        let (default_decision, capabilities) = proxy_fixture();
        let snapshot = PolicyCompiler::compile(CompileRequest::new(
            1,
            1,
            default_decision,
            policies,
            capabilities,
        ));
        let Ok(snapshot) = snapshot else {
            panic!("系统防回环策略编译失败: {snapshot:?}");
        };

        let gateway = PolicyEngine::decide(
            &snapshot,
            &context(
                Platform::MacOs,
                MACOS_GATEWAY_SIGNING_IDENTIFIER,
                Some("TEAM123456"),
            ),
        );
        let forged = PolicyEngine::decide(
            &snapshot,
            &context(Platform::MacOs, MACOS_GATEWAY_SIGNING_IDENTIFIER, None),
        );
        let other = PolicyEngine::decide(
            &snapshot,
            &context(Platform::MacOs, "com.example.browser", None),
        );
        let windows = PolicyEngine::decide(
            &snapshot,
            &context(
                Platform::Windows,
                MACOS_GATEWAY_SIGNING_IDENTIFIER,
                Some("TEAM123456"),
            ),
        );

        assert_eq!(gateway.result().action(), RouteAction::Direct);
        assert_eq!(
            gateway.matched_policy_id().map(PolicyId::as_str),
            Some(MACOS_GATEWAY_POLICY_ID)
        );
        assert_eq!(forged.result().action(), RouteAction::Proxy);
        assert_eq!(other.result().action(), RouteAction::Proxy);
        assert_eq!(windows.result().action(), RouteAction::Proxy);
    }

    #[test]
    fn normalization_replaces_legacy_system_rule_with_current_identity() {
        let legacy = match with_required(&[], &SystemPolicyConfig::default()) {
            Ok(mut value) => match value.pop() {
                Some(policy) => policy,
                None => panic!("旧系统策略缺失"),
            },
            Err(error) => panic!("旧系统策略创建失败: {error}"),
        };
        let config = signed_config();
        let normalized = match with_required(&[legacy], &config) {
            Ok(value) => value,
            Err(error) => panic!("系统策略规范化失败: {error}"),
        };

        assert_eq!(normalized.len(), 1);
        assert!(matches!(contains_required(&normalized, &config), Ok(true)));
        assert!(matches!(
            contains_required(&normalized, &SystemPolicyConfig::default()),
            Ok(false)
        ));
    }

    fn signed_config() -> SystemPolicyConfig {
        match SystemPolicyConfig::new(Some("TEAM123456".to_owned())) {
            Ok(value) => value,
            Err(error) => panic!("签名系统策略配置无效: {error}"),
        }
    }

    fn proxy_fixture() -> (nonproxy_model::DecisionSpec, CompileCapabilities) {
        let outbound = match nonproxy_model::OutboundId::new("primary") {
            Ok(value) => value,
            Err(error) => panic!("测试出口标识无效: {error}"),
        };
        let decision = match nonproxy_model::DecisionSpec::new(
            RouteAction::Proxy,
            Some(outbound.clone()),
            nonproxy_model::FailureMode::Closed,
        ) {
            Ok(value) => value,
            Err(error) => panic!("测试默认代理无效: {error}"),
        };
        let capabilities =
            CompileCapabilities::full().with_outbound(outbound, OutboundCapabilities::full());
        (decision, capabilities)
    }

    fn context(platform: Platform, stable_id: &str, signer_id: Option<&str>) -> ConnectionContext {
        let mut app = match AppIdentity::new(platform, stable_id) {
            Ok(value) => value,
            Err(error) => panic!("测试应用身份无效: {error}"),
        };
        if let Some(signer_id) = signer_id {
            app = match app.with_signer_id(signer_id) {
                Ok(value) => value,
                Err(error) => panic!("测试签名身份无效: {error}"),
            };
        }
        let destination = match Destination::new(Some("example.com"), None, 443, Transport::Tcp) {
            Ok(value) => value,
            Err(error) => panic!("测试目标无效: {error}"),
        };
        ConnectionContext::new(app, destination)
    }
}
