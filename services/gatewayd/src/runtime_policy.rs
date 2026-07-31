use std::collections::BTreeMap;

use nonproxy_model::{Policy, PolicyId};
use nonproxy_storage::SnapshotRecord;

use crate::{GatewayError, snapshot_payload, system_policies};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimePolicyState {
    Draft,
    Pending,
    Active,
    PendingRemoval,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimePolicyRecord {
    policy: Policy,
    state: RuntimePolicyState,
    target_snapshot_version: Option<u64>,
    effective_revision: Option<u64>,
    pending_revision: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimePolicyCatalog {
    generation: u64,
    active_snapshot_version: Option<u64>,
    pending_snapshot_version: Option<u64>,
    records: Vec<RuntimePolicyRecord>,
}

impl RuntimePolicyRecord {
    #[must_use]
    pub const fn policy(&self) -> &Policy {
        &self.policy
    }

    #[must_use]
    pub const fn state(&self) -> RuntimePolicyState {
        self.state
    }

    #[must_use]
    pub const fn target_snapshot_version(&self) -> Option<u64> {
        self.target_snapshot_version
    }

    #[must_use]
    pub const fn effective_revision(&self) -> Option<u64> {
        self.effective_revision
    }

    #[must_use]
    pub const fn pending_revision(&self) -> Option<u64> {
        self.pending_revision
    }
}

impl RuntimePolicyCatalog {
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn active_snapshot_version(&self) -> Option<u64> {
        self.active_snapshot_version
    }

    #[must_use]
    pub const fn pending_snapshot_version(&self) -> Option<u64> {
        self.pending_snapshot_version
    }

    #[must_use]
    pub fn records(&self) -> &[RuntimePolicyRecord] {
        &self.records
    }
}

pub(crate) fn build_runtime_catalog(
    generation: u64,
    current: Vec<Policy>,
    active: Option<&SnapshotRecord>,
    pending: Option<&SnapshotRecord>,
) -> Result<RuntimePolicyCatalog, GatewayError> {
    let active_snapshot_version = snapshot_version(active);
    let pending_snapshot_version = snapshot_version(pending);
    let current = policy_map(current);
    let active_policies = snapshot_policy_map(active)?;
    let pending_policies = snapshot_policy_map(pending)?;
    let pending_is_rollback =
        pending.is_some_and(|record| record.source_snapshot_version().is_some());
    let mut records = Vec::new();

    for (id, policy) in &current {
        let active_policy = active_policies.get(id);
        let pending_policy = pending_policies.get(id);
        let effective_revision = active_policy.map(Policy::revision);
        let pending_revision = pending_policy.map(Policy::revision);
        let (state, target_snapshot_version) = if same_revision(Some(policy), pending_policy) {
            (RuntimePolicyState::Pending, pending_snapshot_version)
        } else if pending.is_some() && pending_policy.is_none() && active_policy.is_some() {
            (RuntimePolicyState::PendingRemoval, pending_snapshot_version)
        } else if same_revision(Some(policy), active_policy) && pending.is_none() {
            (RuntimePolicyState::Active, active_snapshot_version)
        } else {
            (RuntimePolicyState::Draft, None)
        };
        records.push(RuntimePolicyRecord {
            policy: policy.clone(),
            state,
            target_snapshot_version,
            effective_revision,
            pending_revision,
        });
    }

    for (id, policy) in &active_policies {
        if current.contains_key(id) {
            continue;
        }
        let pending_policy = pending_policies.get(id);
        let pending_revision = pending_policy.map(Policy::revision);
        let (state, target_snapshot_version) = match (pending, pending_policy) {
            (_, Some(_)) if pending_is_rollback => {
                (RuntimePolicyState::Pending, pending_snapshot_version)
            }
            (_, Some(_)) => (RuntimePolicyState::PendingRemoval, None),
            (Some(_), None) => (RuntimePolicyState::PendingRemoval, pending_snapshot_version),
            (None, None) => (RuntimePolicyState::PendingRemoval, None),
        };
        records.push(RuntimePolicyRecord {
            policy: policy.clone(),
            state,
            target_snapshot_version,
            effective_revision: Some(policy.revision()),
            pending_revision,
        });
    }

    for (id, policy) in pending_policies {
        if current.contains_key(&id) || active_policies.contains_key(&id) {
            continue;
        }
        let (state, target_snapshot_version) = if pending_is_rollback {
            (RuntimePolicyState::Pending, pending_snapshot_version)
        } else {
            // 普通发布后又删除草稿时，旧待确认快照不能被误认为删除已经发布。
            (RuntimePolicyState::PendingRemoval, None)
        };
        records.push(RuntimePolicyRecord {
            pending_revision: Some(policy.revision()),
            policy,
            state,
            target_snapshot_version,
            effective_revision: None,
        });
    }
    records.sort_by(|left, right| left.policy.id().cmp(right.policy.id()));
    Ok(RuntimePolicyCatalog {
        generation,
        active_snapshot_version,
        pending_snapshot_version,
        records,
    })
}

fn snapshot_policy_map(
    record: Option<&SnapshotRecord>,
) -> Result<BTreeMap<PolicyId, Policy>, GatewayError> {
    match record {
        Some(record) => {
            let (policies, _capabilities, _default) =
                snapshot_payload::decode(record.artifact().payload())?;
            Ok(policy_map(policies))
        }
        None => Ok(BTreeMap::new()),
    }
}

fn policy_map(policies: Vec<Policy>) -> BTreeMap<PolicyId, Policy> {
    policies
        .into_iter()
        .filter(|policy| !system_policies::is_managed_system_policy(policy))
        .map(|policy| (policy.id().clone(), policy))
        .collect()
}

fn same_revision(left: Option<&Policy>, right: Option<&Policy>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left.revision() == right.revision())
}

fn snapshot_version(record: Option<&SnapshotRecord>) -> Option<u64> {
    record.map(|record| record.artifact().snapshot_version())
}
