use std::time::{SystemTime, UNIX_EPOCH};

use nonproxy_adapter_transaction::{
    AdapterInstallation as TransactionInstallation, IntegratedPreparation,
};
use nonproxy_proto::{
    adapter::v1::{
        PrepareChangeRequest, PrepareChangeResponse, VerifyChangeRequest, VerifyChangeResponse,
    },
    common::v1::EvidenceLevel,
};
use prost_types::Timestamp;
use sha2::{Digest, Sha256};
use tonic::{Response, Status};

use crate::{
    AdapterHostError, candidate_validation::validate_integrated, capabilities::capabilities,
    detection::detect, mapping::error_detail, rpc_state::AdapterRpcService,
};

impl AdapterRpcService {
    pub(crate) async fn prepare_change_rpc(
        &self,
        request: PrepareChangeRequest,
    ) -> Result<Response<PrepareChangeResponse>, Status> {
        let operation_id = self
            .authenticate(request.context.as_ref(), Some(&request.operation_id))?
            .to_owned();
        let _mutation = self.mutation_gate.lock().await;
        let result = self.prepare_change(request, operation_id).await;
        Ok(Response::new(match result {
            Ok(value) => PrepareChangeResponse {
                change_id: value.change_id,
                backup_id: value.backup_id,
                candidate_hash: value.candidate_sha256.to_vec(),
                expires_at: Some(timestamp_from_millis(value.expires_at_unix_ms)?),
                error: None,
                rule_count: u32::try_from(value.rule_count)
                    .map_err(|_| Status::internal("适配器规则数量溢出"))?,
                client_validated: true,
                configuration_candidate_hash: value
                    .configuration_candidate_sha256
                    .map_or_else(Vec::new, |hash| hash.to_vec()),
                managed_rules_reference: value.managed_rules_reference.unwrap_or_default(),
                direct_target: value.direct_target.unwrap_or_default(),
            },
            Err(error) => PrepareChangeResponse {
                change_id: String::new(),
                backup_id: String::new(),
                candidate_hash: Vec::new(),
                expires_at: None,
                error: Some(error_detail(&error)),
                rule_count: 0,
                client_validated: false,
                configuration_candidate_hash: Vec::new(),
                managed_rules_reference: String::new(),
                direct_target: String::new(),
            },
        }))
    }

    pub(crate) async fn verify_change_rpc(
        &self,
        request: VerifyChangeRequest,
    ) -> Result<Response<VerifyChangeResponse>, Status> {
        self.authenticate(request.context.as_ref(), Some(&request.operation_id))?;
        let _mutation = self.mutation_gate.lock().await;
        let transactions = self.transactions.clone();
        let change_id = request.change_id;
        let verified = tokio::task::spawn_blocking(move || transactions.verify(&change_id))
            .await
            .map_err(|_| Status::internal("适配器事务任务失败"))?;
        Ok(Response::new(match verified {
            Ok(value) => VerifyChangeResponse {
                verified: value.path_verified,
                evidence_level: if value.configuration_verified {
                    EvidenceLevel::Configuration.into()
                } else {
                    EvidenceLevel::Unspecified.into()
                },
                error: None,
                configuration_verified: value.configuration_verified,
                path_verified: value.path_verified,
            },
            Err(error) => VerifyChangeResponse {
                verified: false,
                evidence_level: EvidenceLevel::Unspecified.into(),
                error: Some(error_detail(&error.into())),
                configuration_verified: false,
                path_verified: false,
            },
        }))
    }

    async fn prepare_change(
        &self,
        request: PrepareChangeRequest,
        operation_id: String,
    ) -> Result<nonproxy_adapter_transaction::PreparedChange, AdapterHostError> {
        let expected_hash: [u8; 32] = request
            .normalized_policy_hash
            .as_slice()
            .try_into()
            .map_err(|_| AdapterHostError::PolicyHashMismatch)?;
        let actual_hash: [u8; 32] = Sha256::digest(&request.normalized_policy).into();
        if actual_hash != expected_hash {
            return Err(AdapterHostError::PolicyHashMismatch);
        }
        if !request.installation_id.is_empty() && request.installation_id != request.adapter_id {
            return Err(AdapterHostError::InstallationInvalid);
        }
        let installation = self.catalog.get(&request.adapter_id)?;
        let detected = detect(installation.client, &installation.executable_path).await?;
        if !detected.supported() || capabilities(detected.client, detected.version).is_empty() {
            return Err(AdapterHostError::ClientUnsupported);
        }
        let transaction_installation = TransactionInstallation::new(
            installation.adapter_id.clone(),
            installation.client,
            detected.version,
            installation.managed_rules_path.clone(),
        );
        let main_configuration_path = installation
            .main_configuration_path
            .clone()
            .ok_or(AdapterHostError::InstallationIncomplete)?;
        let transactions = self.transactions.clone();
        let policy = request.normalized_policy;
        let preview_installation = transaction_installation.clone();
        let preview_policy = policy.clone();
        let preview_transactions = transactions.clone();
        let preview_configuration_path = main_configuration_path.clone();
        let preview_direct_target = installation.direct_target.clone();
        let preview = tokio::task::spawn_blocking(move || {
            nonproxy_adapter_transaction::AdapterTransactionManager::preview_integrated(
                &preview_installation,
                &preview_configuration_path,
                preview_direct_target,
                &preview_policy,
            )
        })
        .await
        .map_err(AdapterHostError::Task)??;
        validate_integrated(&detected, &main_configuration_path, &preview).await?;
        let preview_hash = *preview.rendered_rules().sha256();
        let preview_configuration_hash = *preview.configuration_sha256();
        let preview_rule_count = preview.rendered_rules().rule_count();
        let preview_reference = preview.managed_rules_reference().to_owned();
        let preview_direct_target = preview.direct_target().to_owned();
        let now = now_unix_millis()?;
        let prepared = tokio::task::spawn_blocking(move || {
            let request = IntegratedPreparation::new(
                &transaction_installation,
                &main_configuration_path,
                &operation_id,
                &policy,
                &preview_hash,
                &preview_configuration_hash,
                now,
            )
            .with_direct_target(installation.direct_target);
            transactions.prepare_integrated(request)
        })
        .await
        .map_err(AdapterHostError::Task)?
        .map_err(AdapterHostError::from)?;
        if prepared.candidate_sha256 != preview_hash
            || prepared.configuration_candidate_sha256 != Some(preview_configuration_hash)
            || prepared.rule_count != preview_rule_count
            || prepared.managed_rules_reference.as_deref() != Some(preview_reference.as_str())
            || prepared.direct_target.as_deref() != Some(preview_direct_target.as_str())
        {
            let change_id = prepared.change_id.clone();
            tokio::task::spawn_blocking(move || preview_transactions.remove_change(&change_id))
                .await
                .map_err(AdapterHostError::Task)??;
            return Err(AdapterHostError::Transaction(
                nonproxy_adapter_transaction::AdapterTransactionError::StateCorrupt,
            ));
        }
        Ok(prepared)
    }
}

pub(crate) fn now_unix_millis() -> Result<u64, AdapterHostError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AdapterHostError::Configuration)?
        .as_millis();
    u64::try_from(millis).map_err(|_| AdapterHostError::Configuration)
}

fn timestamp_from_millis(value: u64) -> Result<Timestamp, Status> {
    let seconds = value / 1_000;
    let nanos = (value % 1_000) * 1_000_000;
    Ok(Timestamp {
        seconds: i64::try_from(seconds).map_err(|_| Status::internal("适配器时间溢出"))?,
        nanos: i32::try_from(nanos).map_err(|_| Status::internal("适配器时间溢出"))?,
    })
}
