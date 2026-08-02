use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("标识符不能为空")]
    EmptyIdentifier,
    #[error("标识符超过长度上限")]
    IdentifierTooLong,
    #[error("标识符包含控制字符")]
    IdentifierContainsControl,
    #[error("标识符包含不允许的字符")]
    InvalidIdentifierCharacter,
    #[error("应用稳定标识不能为空")]
    EmptyAppStableId,
    #[error("应用身份字段超过长度上限")]
    AppIdentityFieldTooLong,
    #[error("应用身份字段不能为空或包含控制字符")]
    InvalidAppIdentityField,
    #[error("域名不能为空")]
    EmptyDomain,
    #[error("域名不能是 IP 地址")]
    DomainIsIpAddress,
    #[error("域名包含无效的 IDNA 内容")]
    InvalidIdnaDomain(#[source] idna::Errors),
    #[error("域名首尾不能包含空白字符")]
    DomainHasOuterWhitespace,
    #[error("域名超过 DNS 长度上限")]
    DomainTooLong,
    #[error("域名标签无效")]
    InvalidDomainLabel,
    #[error("可注册域规则必须使用可注册域本身")]
    InvalidRegistrableDomainPattern,
    #[error("目标必须包含域名或 IP 地址")]
    DestinationMissingAddress,
    #[error("目标端口必须大于零")]
    InvalidDestinationPort,
    #[error("CIDR 前缀长度无效")]
    InvalidCidrPrefix,
    #[error("CIDR 必须使用地址加斜杠前缀格式")]
    InvalidCidrShape,
    #[error("CIDR 地址文本格式无效")]
    InvalidCidrAddress(#[source] std::net::AddrParseError),
    #[error("CIDR 前缀文本格式无效")]
    InvalidCidrPrefixText(#[source] std::num::ParseIntError),
    #[error("端口范围无效")]
    InvalidPortRange,
    #[error("代理决策必须指定出口")]
    ProxyDecisionMissingOutbound,
    #[error("直连或阻断决策不能指定出口")]
    NonProxyDecisionHasOutbound,
    #[error("出口组快照必须包含有效修订和 2 到 32 个不重复成员")]
    InvalidOutboundGroupSpec,
    #[error("策略显示名称不能为空")]
    EmptyPolicyDisplayName,
    #[error("策略显示名称超过长度上限")]
    PolicyDisplayNameTooLong,
    #[error("策略显示名称包含控制字符")]
    InvalidPolicyDisplayName,
    #[error("高信任策略来源与内容来源不一致")]
    InvalidPolicyOrigin,
    #[error("策略不能同时匹配域名和 CIDR")]
    AmbiguousDestinationMatcher,
    #[error("网络配置档规则不能与其他匹配维度组合")]
    NetworkMatcherCannotBeCombined,
    #[error("应用目标规则必须同时包含应用和目标")]
    AppDestinationMatcherIncomplete,
    #[error("应用规则只能包含应用匹配条件")]
    AppMatcherHasExtraDimensions,
    #[error("站点规则必须只包含域名匹配条件")]
    SiteMatcherInvalid,
    #[error("CIDR 规则必须只包含 CIDR 匹配条件")]
    CidrMatcherInvalid,
    #[error("网络规则必须只包含网络配置档匹配条件")]
    NetworkMatcherInvalid,
    #[error("系统或内置规则不能包含网络配置档匹配条件")]
    GlobalRuleHasNetworkMatcher,
    #[error("适配器规则必须包含明确的应用或目标匹配条件")]
    AdapterMatcherInvalid,
    #[error("端口范围存在重叠")]
    OverlappingPortRanges,
    #[error("网络指纹必须是受支持的脱敏值")]
    InvalidNetworkFingerprint,
    #[error("网络配置档显示名称无效")]
    InvalidNetworkProfileDisplayName,
    #[error("网络配置档修订必须大于零")]
    InvalidNetworkProfileRevision,
    #[error("运行态覆盖的代理模式必须指定出口")]
    RuntimeOverrideProxyMissingOutbound,
    #[error("暂停或直连运行态覆盖不能指定出口")]
    RuntimeOverrideNonProxyHasOutbound,
    #[error("运行态覆盖到期时间必须大于零")]
    RuntimeOverrideExpiryInvalid,
}

impl ModelError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyIdentifier => "NP_MODEL_IDENTIFIER_EMPTY",
            Self::IdentifierTooLong => "NP_MODEL_IDENTIFIER_TOO_LONG",
            Self::IdentifierContainsControl => "NP_MODEL_IDENTIFIER_CONTROL_CHARACTER",
            Self::InvalidIdentifierCharacter => "NP_MODEL_IDENTIFIER_CHARACTER_INVALID",
            Self::EmptyAppStableId => "NP_MODEL_APP_STABLE_ID_EMPTY",
            Self::AppIdentityFieldTooLong => "NP_MODEL_APP_IDENTITY_FIELD_TOO_LONG",
            Self::InvalidAppIdentityField => "NP_MODEL_APP_IDENTITY_FIELD_INVALID",
            Self::EmptyDomain => "NP_MODEL_DOMAIN_EMPTY",
            Self::DomainIsIpAddress => "NP_MODEL_DOMAIN_IS_IP",
            Self::InvalidIdnaDomain(_) | Self::DomainHasOuterWhitespace => {
                "NP_MODEL_DOMAIN_IDNA_INVALID"
            }
            Self::DomainTooLong => "NP_MODEL_DOMAIN_TOO_LONG",
            Self::InvalidDomainLabel => "NP_MODEL_DOMAIN_LABEL_INVALID",
            Self::InvalidRegistrableDomainPattern => "NP_MODEL_REGISTRABLE_DOMAIN_PATTERN_INVALID",
            Self::DestinationMissingAddress => "NP_MODEL_DESTINATION_ADDRESS_MISSING",
            Self::InvalidDestinationPort => "NP_MODEL_DESTINATION_PORT_INVALID",
            Self::InvalidCidrPrefix => "NP_MODEL_CIDR_PREFIX_INVALID",
            Self::InvalidCidrShape
            | Self::InvalidCidrAddress(_)
            | Self::InvalidCidrPrefixText(_) => "NP_MODEL_CIDR_TEXT_INVALID",
            Self::InvalidPortRange => "NP_MODEL_PORT_RANGE_INVALID",
            Self::ProxyDecisionMissingOutbound => "NP_MODEL_PROXY_OUTBOUND_MISSING",
            Self::NonProxyDecisionHasOutbound => "NP_MODEL_NON_PROXY_OUTBOUND_PRESENT",
            Self::InvalidOutboundGroupSpec => "NP_MODEL_OUTBOUND_GROUP_SPEC_INVALID",
            Self::EmptyPolicyDisplayName => "NP_MODEL_POLICY_NAME_EMPTY",
            Self::PolicyDisplayNameTooLong => "NP_MODEL_POLICY_NAME_TOO_LONG",
            Self::InvalidPolicyDisplayName => "NP_MODEL_POLICY_NAME_INVALID",
            Self::InvalidPolicyOrigin => "NP_MODEL_POLICY_ORIGIN_INVALID",
            Self::AmbiguousDestinationMatcher => "NP_MODEL_DESTINATION_MATCHER_AMBIGUOUS",
            Self::NetworkMatcherCannotBeCombined => "NP_MODEL_NETWORK_MATCHER_COMBINED",
            Self::AppDestinationMatcherIncomplete => "NP_MODEL_APP_DESTINATION_MATCHER_INCOMPLETE",
            Self::AppMatcherHasExtraDimensions => "NP_MODEL_APP_MATCHER_EXTRA_DIMENSION",
            Self::SiteMatcherInvalid => "NP_MODEL_SITE_MATCHER_INVALID",
            Self::CidrMatcherInvalid => "NP_MODEL_CIDR_MATCHER_INVALID",
            Self::NetworkMatcherInvalid => "NP_MODEL_NETWORK_MATCHER_INVALID",
            Self::GlobalRuleHasNetworkMatcher => "NP_MODEL_GLOBAL_RULE_NETWORK_MATCHER",
            Self::AdapterMatcherInvalid => "NP_MODEL_ADAPTER_MATCHER_INVALID",
            Self::OverlappingPortRanges => "NP_MODEL_PORT_RANGE_OVERLAP",
            Self::InvalidNetworkFingerprint => "NP_MODEL_NETWORK_FINGERPRINT_INVALID",
            Self::InvalidNetworkProfileDisplayName => "NP_MODEL_NETWORK_PROFILE_NAME_INVALID",
            Self::InvalidNetworkProfileRevision => "NP_MODEL_NETWORK_PROFILE_REVISION_INVALID",
            Self::RuntimeOverrideProxyMissingOutbound => {
                "NP_MODEL_RUNTIME_OVERRIDE_PROXY_OUTBOUND_MISSING"
            }
            Self::RuntimeOverrideNonProxyHasOutbound => {
                "NP_MODEL_RUNTIME_OVERRIDE_NON_PROXY_OUTBOUND_PRESENT"
            }
            Self::RuntimeOverrideExpiryInvalid => "NP_MODEL_RUNTIME_OVERRIDE_EXPIRY_INVALID",
        }
    }
}
