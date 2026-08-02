use nonproxy_model::Decision;
use nonproxy_policy::CompiledPolicySnapshot;
use nonproxy_storage::DecisionEvidence;

use crate::GatewayError;

pub(crate) fn validate_group_evidence(
    snapshot: &CompiledPolicySnapshot,
    decision: &Decision,
    evidence: &DecisionEvidence,
) -> Result<(), GatewayError> {
    let Some(group_id) = decision.result().outbound_group_id() else {
        return Ok(());
    };
    let Some(outbound_id) = evidence.outbound_id() else {
        return Ok(());
    };
    let valid = snapshot
        .outbound_groups()
        .get(group_id)
        .is_some_and(|group| group.members().contains(outbound_id));
    if !valid {
        return Err(GatewayError::InvalidRequest(
            "Provider 上报的实际出口不属于判定出口组",
        ));
    }
    Ok(())
}
