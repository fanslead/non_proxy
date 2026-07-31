use nonproxy_storage::StorageError;
use tonic::Status;

use crate::{GatewayError, control_rpc_helpers::internal_status};

pub fn status(error: GatewayError) -> Status {
    match error {
        invalid @ (GatewayError::Model(_)
        | GatewayError::InvalidRequest(_)
        | GatewayError::Storage(
            StorageError::ConnectionDecisionInvalid
            | StorageError::DecisionEvidenceInvalid
            | StorageError::ConnectionDecisionReplayMismatch,
        )) => Status::invalid_argument(format!("{}: {}", invalid.code(), invalid)),
        internal => internal_status(internal),
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_storage::StorageError;
    use tonic::Code;

    use super::status;
    use crate::GatewayError;

    #[test]
    fn invalid_evidence_is_a_client_error_but_database_failure_is_not() {
        assert_eq!(
            status(GatewayError::Storage(StorageError::DecisionEvidenceInvalid)).code(),
            Code::InvalidArgument
        );
        assert_eq!(
            status(GatewayError::StateLockPoisoned("测试")).code(),
            Code::Internal
        );
    }
}
