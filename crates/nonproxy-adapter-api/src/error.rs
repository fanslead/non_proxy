use thiserror::Error;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AdapterContractError {
    #[error("适配器策略超过大小上限")]
    PolicyTooLarge,
    #[error("适配器策略不是有效 JSON")]
    PolicyInvalid,
    #[error("适配器策略格式版本不受支持")]
    PolicyVersionUnsupported,
    #[error("适配器策略修订号无效")]
    PolicyRevisionInvalid,
    #[error("适配器策略规则数量超过上限")]
    RuleLimitExceeded,
    #[error("适配器策略包含重复规则标识")]
    DuplicateRuleId,
    #[error("适配器规则标识无效")]
    RuleIdInvalid,
    #[error("适配器规则动作不受支持")]
    ActionUnsupported,
    #[error("适配器规则选择器无效")]
    SelectorInvalid,
    #[error("当前客户端版本不支持该规则")]
    ClientVersionUnsupported,
    #[error("渲染结果超过大小上限")]
    RenderedRulesTooLarge,
}
