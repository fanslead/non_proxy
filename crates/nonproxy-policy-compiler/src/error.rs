use nonproxy_model::{ModelError, PolicyId};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyConflict {
    code: &'static str,
    message: String,
    policy_ids: Vec<PolicyId>,
}

impl PolicyConflict {
    #[must_use]
    pub fn for_policy(code: &'static str, message: impl Into<String>, policy_id: PolicyId) -> Self {
        Self {
            code,
            message: message.into(),
            policy_ids: vec![policy_id],
        }
    }

    #[must_use]
    pub fn for_policies(
        code: &'static str,
        message: impl Into<String>,
        policy_ids: Vec<PolicyId>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            policy_ids,
        }
    }

    #[must_use]
    pub fn global(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            policy_ids: Vec::new(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn policy_ids(&self) -> &[PolicyId] {
        &self.policy_ids
    }
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("策略编译校验失败")]
    Validation { conflicts: Vec<PolicyConflict> },
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl CompileError {
    #[must_use]
    pub fn conflicts(&self) -> &[PolicyConflict] {
        match self {
            Self::Validation { conflicts } => conflicts,
            Self::Model(_) => &[],
        }
    }
}
