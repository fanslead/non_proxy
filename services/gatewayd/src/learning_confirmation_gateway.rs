use std::collections::BTreeSet;

use nonproxy_learning::{ConfirmationId, LearningSessionId};
use nonproxy_model::{
    DecisionSpec, DomainMatchKind, DomainMatcher, DomainName, Policy, PolicyId, PolicyMatch,
    PolicyMetadata, PolicyOrigin, PolicySourceKind, RouteAction,
};
use nonproxy_storage::{
    LearningConfirmationReceipt, LearningPolicySelection, OutboundReference, SnapshotRecord,
    StorageError,
};

use crate::{
    Gateway, GatewayError, clock::unix_time_ms, gateway::PublishedSnapshot, outbound_capabilities,
    snapshot_builder::build_snapshot, snapshot_payload,
};

#[derive(Debug)]
pub struct LearningConfirmationResult {
    receipt: LearningConfirmationReceipt,
    snapshot: SnapshotRecord,
    replayed: bool,
    snapshot_staged: bool,
}

impl LearningConfirmationResult {
    #[must_use]
    pub const fn receipt(&self) -> &LearningConfirmationReceipt {
        &self.receipt
    }

    #[must_use]
    pub const fn snapshot(&self) -> &SnapshotRecord {
        &self.snapshot
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }

    #[must_use]
    pub const fn snapshot_staged(&self) -> bool {
        self.snapshot_staged
    }
}

struct ConfirmationState {
    receipt: Option<LearningConfirmationReceipt>,
    policies: Vec<Policy>,
    outbounds: Vec<OutboundReference>,
    pending: Option<SnapshotRecord>,
    latest_snapshot_version: u64,
}

impl Gateway {
    pub async fn confirm_learning_candidates(
        &self,
        session_id: LearningSessionId,
        confirmation_id: ConfirmationId,
        selected_domains: Vec<DomainName>,
    ) -> Result<LearningConfirmationResult, GatewayError> {
        let selected_domains = validate_selected_domains(selected_domains)?;
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let lookup_id = confirmation_id.clone();
        let state = self
            .database
            .run(move |database| {
                Ok(ConfirmationState {
                    receipt: database.learning_confirmations().get(&lookup_id)?,
                    policies: database.policies().list()?,
                    outbounds: database.outbounds().list()?,
                    pending: database.snapshots().pending()?,
                    latest_snapshot_version: database.snapshots().latest_version()?.unwrap_or(0),
                })
            })
            .await?;

        if let Some(receipt) = state.receipt.as_ref() {
            validate_receipt(receipt, &session_id, &selected_domains)?;
            if receipt.snapshot_version().is_none()
                && !receipt_policies_are_present(receipt, &state.policies)
            {
                return Err(StorageError::LearningConfirmationReplayMismatch.into());
            }
            let (snapshot, snapshot_staged) = self
                .ensure_confirmation_snapshot(&confirmation_id, receipt, &state, now)
                .await?;
            return Ok(LearningConfirmationResult {
                receipt: receipt.clone(),
                snapshot,
                replayed: true,
                snapshot_staged,
            });
        }
        if state.pending.is_some() {
            return Err(StorageError::PendingSnapshotExists.into());
        }

        let selections = build_selections(&selected_domains, &state.policies)?;
        let mut proposed = state.policies.clone();
        proposed.extend(
            selections
                .iter()
                .filter(|value| !value.existing())
                .map(|value| value.policy().clone()),
        );
        let next_snapshot_version = next_version(state.latest_snapshot_version)?;
        let published = build_snapshot(
            self.capabilities().clone(),
            &proposed,
            &state.outbounds,
            next_snapshot_version,
            now,
        )?;
        let saved_id = confirmation_id.clone();
        let saved_session = session_id.clone();
        let receipt = self
            .database
            .run(move |database| {
                Ok(database.learning_confirmations().confirm_site(
                    &saved_id,
                    &saved_session,
                    &selections,
                    now,
                )?)
            })
            .await?;
        let replayed = receipt.replayed();
        let snapshot = self
            .stage_and_mark_confirmation(&confirmation_id, published)
            .await?;
        Ok(LearningConfirmationResult {
            receipt,
            snapshot,
            replayed,
            snapshot_staged: true,
        })
    }

    async fn ensure_confirmation_snapshot(
        &self,
        confirmation_id: &ConfirmationId,
        receipt: &LearningConfirmationReceipt,
        state: &ConfirmationState,
        now_unix_ms: u64,
    ) -> Result<(SnapshotRecord, bool), GatewayError> {
        if let Some(version) = receipt.snapshot_version() {
            return Ok((self.snapshot(version).await?, false));
        }
        if let Some(pending) = state.pending.as_ref() {
            if snapshot_contains(
                pending,
                &state.policies,
                &state.outbounds,
                self.capabilities().clone(),
            )? {
                self.mark_confirmation_snapshot(
                    confirmation_id,
                    pending.artifact().snapshot_version(),
                )
                .await?;
                return Ok((pending.clone(), false));
            }
            return Err(StorageError::PendingSnapshotExists.into());
        }
        let published = build_snapshot(
            self.capabilities().clone(),
            &state.policies,
            &state.outbounds,
            next_version(state.latest_snapshot_version)?,
            now_unix_ms,
        )?;
        Ok((
            self.stage_and_mark_confirmation(confirmation_id, published)
                .await?,
            true,
        ))
    }

    async fn stage_and_mark_confirmation(
        &self,
        confirmation_id: &ConfirmationId,
        published: PublishedSnapshot,
    ) -> Result<SnapshotRecord, GatewayError> {
        let artifact = published.artifact().clone();
        let version = artifact.snapshot_version();
        self.database
            .run(move |database| {
                database.snapshots().stage(&artifact)?;
                Ok(())
            })
            .await?;
        self.mark_confirmation_snapshot(confirmation_id, version)
            .await?;
        self.snapshot(version).await
    }

    async fn mark_confirmation_snapshot(
        &self,
        confirmation_id: &ConfirmationId,
        version: u64,
    ) -> Result<(), GatewayError> {
        let confirmation_id = confirmation_id.clone();
        self.database
            .run(move |database| {
                database
                    .learning_confirmations()
                    .mark_snapshot(&confirmation_id, version)?;
                Ok(())
            })
            .await
    }

    async fn snapshot(&self, version: u64) -> Result<SnapshotRecord, GatewayError> {
        self.database
            .run(move |database| {
                database
                    .snapshots()
                    .get(version)?
                    .ok_or_else(|| StorageError::SnapshotNotFound.into())
            })
            .await
    }
}

fn validate_selected_domains(
    selected_domains: Vec<DomainName>,
) -> Result<Vec<DomainName>, GatewayError> {
    if selected_domains.is_empty() || selected_domains.len() > 256 {
        return Err(GatewayError::InvalidRequest(
            "确认域名数量必须在 1 到 256 之间",
        ));
    }
    let unique = selected_domains
        .iter()
        .map(DomainName::as_ascii)
        .collect::<BTreeSet<_>>();
    if unique.len() != selected_domains.len() {
        return Err(GatewayError::InvalidRequest("确认域名不能重复"));
    }
    Ok(selected_domains)
}

fn validate_receipt(
    receipt: &LearningConfirmationReceipt,
    session_id: &LearningSessionId,
    selected_domains: &[DomainName],
) -> Result<(), GatewayError> {
    let expected = receipt
        .policies()
        .iter()
        .map(|value| value.domain().as_ascii())
        .collect::<BTreeSet<_>>();
    let actual = selected_domains
        .iter()
        .map(DomainName::as_ascii)
        .collect::<BTreeSet<_>>();
    if receipt.session_id() != session_id || expected != actual {
        return Err(StorageError::LearningConfirmationReplayMismatch.into());
    }
    Ok(())
}

fn receipt_policies_are_present(
    receipt: &LearningConfirmationReceipt,
    policies: &[Policy],
) -> bool {
    receipt.policies().iter().all(|confirmed| {
        policies.iter().any(|policy| {
            policy.id() == confirmed.policy_id() && is_reusable_policy(policy, confirmed.domain())
        })
    })
}

fn build_selections(
    domains: &[DomainName],
    policies: &[Policy],
) -> Result<Vec<LearningPolicySelection>, GatewayError> {
    domains
        .iter()
        .map(|domain| {
            if let Some(policy) = policies
                .iter()
                .find(|policy| is_reusable_policy(policy, domain))
            {
                return Ok(LearningPolicySelection::new(
                    domain.clone(),
                    policy.clone(),
                    true,
                ));
            }
            Ok(LearningPolicySelection::new(
                domain.clone(),
                direct_site_policy(domain.clone())?,
                false,
            ))
        })
        .collect()
}

fn is_reusable_policy(policy: &Policy, domain: &DomainName) -> bool {
    let matcher = policy.matcher();
    policy.enabled()
        && policy.source_kind() == PolicySourceKind::Site
        && policy.decision().action() == RouteAction::Direct
        && matcher.app().is_none()
        && matcher.cidr().is_none()
        && matcher.network().is_none()
        && matcher.transports().is_empty()
        && matcher.ports().is_empty()
        && matcher.domain().is_some_and(|value| {
            value.kind() == DomainMatchKind::Exact && value.pattern() == domain
        })
}

fn direct_site_policy(domain: DomainName) -> Result<Policy, GatewayError> {
    let matcher = DomainMatcher::new(DomainMatchKind::Exact, domain.as_ascii())?;
    let policy_match = PolicyMatch::new(None, Some(matcher), None, None, Vec::new(), Vec::new())?;
    Policy::new(
        new_policy_id()?,
        format!("直连 {}", domain.as_ascii()),
        policy_match,
        DecisionSpec::direct(),
        PolicyMetadata::new(PolicySourceKind::Site, 100, PolicyOrigin::User, 1),
    )
    .map_err(GatewayError::from)
}

fn new_policy_id() -> Result<PolicyId, GatewayError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| GatewayError::Random(error.to_string()))?;
    let suffix = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    PolicyId::new(format!("learning-{suffix}")).map_err(GatewayError::from)
}

fn next_version(current: u64) -> Result<u64, GatewayError> {
    current
        .checked_add(1)
        .ok_or(GatewayError::SnapshotVersionExhausted)
}

fn snapshot_contains(
    snapshot: &SnapshotRecord,
    policies: &[Policy],
    outbounds: &[OutboundReference],
    capabilities: nonproxy_policy_compiler::CompileCapabilities,
) -> Result<bool, GatewayError> {
    let capabilities = outbound_capabilities::for_configured_outbounds(capabilities, outbounds);
    let expected = snapshot_payload::encode(policies, &capabilities, &DecisionSpec::direct())?;
    Ok(snapshot.artifact().payload() == expected)
}
