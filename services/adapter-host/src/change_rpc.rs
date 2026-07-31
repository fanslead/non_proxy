use std::time::{SystemTime, UNIX_EPOCH};

use nonproxy_adapter_transaction::AdapterInstallation as TransactionInstallation;
use nonproxy_proto::{
    adapter::v1::{
        ApplyChangeRequest, ApplyChangeResponse, PrepareChangeRequest, PrepareChangeResponse,
        RollbackChangeRequest, RollbackChangeResponse, VerifyChangeRequest, VerifyChangeResponse,
    },
    common::v1::EvidenceLevel,
};
use prost_types::Timestamp;
use sha2::{Digest, Sha256};
use tonic::{Response, Status};

use crate::{
    AdapterHostError, capabilities::capabilities, detection::detect, mapping::error_detail,
    rpc_state::AdapterRpcService,
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
            },
            Err(error) => PrepareChangeResponse {
                change_id: String::new(),
                backup_id: String::new(),
                candidate_hash: Vec::new(),
                expires_at: None,
                error: Some(error_detail(&error)),
                rule_count: 0,
            },
        }))
    }

    pub(crate) async fn apply_change_rpc(
        &self,
        request: ApplyChangeRequest,
    ) -> Result<Response<ApplyChangeResponse>, Status> {
        self.authenticate(request.context.as_ref(), Some(&request.operation_id))?;
        let _mutation = self.mutation_gate.lock().await;
        if let Err(error) = self.validate_prepared_client(&request.change_id).await {
            return Ok(Response::new(ApplyChangeResponse {
                applied: false,
                reloaded: false,
                error: Some(error_detail(&error)),
                replayed: false,
            }));
        }
        let transactions = self.transactions.clone();
        let change_id = request.change_id;
        let expected_hash = request.expected_candidate_hash;
        let now = now_unix_millis().map_err(|_| Status::internal("系统时间无效"))?;
        let applied = tokio::task::spawn_blocking(move || {
            transactions.apply(&change_id, &expected_hash, now)
        })
        .await
        .map_err(|_| Status::internal("适配器事务任务失败"))?;
        Ok(Response::new(match applied {
            Ok(value) => ApplyChangeResponse {
                applied: value.applied,
                reloaded: false,
                error: None,
                replayed: value.replayed,
            },
            Err(error) => ApplyChangeResponse {
                applied: false,
                reloaded: false,
                error: Some(error_detail(&error.into())),
                replayed: false,
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

    pub(crate) async fn rollback_change_rpc(
        &self,
        request: RollbackChangeRequest,
    ) -> Result<Response<RollbackChangeResponse>, Status> {
        self.authenticate(request.context.as_ref(), Some(&request.operation_id))?;
        let _mutation = self.mutation_gate.lock().await;
        let transactions = self.transactions.clone();
        let change_id = request.change_id;
        let backup_id = request.backup_id;
        let rolled_back =
            tokio::task::spawn_blocking(move || transactions.rollback(&change_id, &backup_id))
                .await
                .map_err(|_| Status::internal("适配器事务任务失败"))?;
        Ok(Response::new(match rolled_back {
            Ok(value) => RollbackChangeResponse {
                restored: value.restored,
                reloaded: false,
                error: None,
                replayed: value.replayed,
            },
            Err(error) => RollbackChangeResponse {
                restored: false,
                reloaded: false,
                error: Some(error_detail(&error.into())),
                replayed: false,
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
            installation.adapter_id,
            installation.client,
            detected.version,
            installation.managed_rules_path,
        );
        let transactions = self.transactions.clone();
        let policy = request.normalized_policy;
        let now = now_unix_millis()?;
        tokio::task::spawn_blocking(move || {
            transactions.prepare(&transaction_installation, &operation_id, &policy, now)
        })
        .await
        .map_err(AdapterHostError::Task)?
        .map_err(AdapterHostError::from)
    }

    async fn validate_prepared_client(&self, change_id: &str) -> Result<(), AdapterHostError> {
        let transactions = self.transactions.clone();
        let change_id = change_id.to_owned();
        let expected =
            tokio::task::spawn_blocking(move || transactions.change_installation(&change_id))
                .await
                .map_err(AdapterHostError::Task)??;
        let registered = self.catalog.get(&expected.adapter_id)?;
        if registered.client != expected.client {
            return Err(AdapterHostError::ClientVersionChanged);
        }
        let detected = detect(registered.client, &registered.executable_path).await?;
        if detected.version != expected.client_version || !detected.supported() {
            return Err(AdapterHostError::ClientVersionChanged);
        }
        Ok(())
    }
}

fn now_unix_millis() -> Result<u64, AdapterHostError> {
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
