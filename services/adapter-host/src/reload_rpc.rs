use nonproxy_adapter_transaction::{ApplyOutcome, ChangeInstallation};
use nonproxy_proto::adapter::v1::{
    ApplyChangeRequest, ApplyChangeResponse, RollbackChangeRequest, RollbackChangeResponse,
};
use tonic::{Response, Status};

use crate::{
    AdapterHostError,
    capabilities::hot_reload_supported,
    detection::{DetectedClient, detect},
    mapping::error_detail,
    reload::ReloadPlan,
    rpc_state::AdapterRpcService,
};

struct ValidatedChange {
    change: ChangeInstallation,
    detected: DetectedClient,
}

impl AdapterRpcService {
    pub(crate) async fn apply_change_rpc(
        &self,
        request: ApplyChangeRequest,
    ) -> Result<Response<ApplyChangeResponse>, Status> {
        self.authenticate(request.context.as_ref(), Some(&request.operation_id))?;
        let _mutation = self.mutation_gate.lock().await;
        let validated = match self.validate_prepared_client(&request.change_id).await {
            Ok(value) => value,
            Err(error) => return Ok(Response::new(apply_error(error))),
        };
        if let Err(error) = self
            .preflight_transaction(&request, &validated.change)
            .await?
        {
            return Ok(Response::new(apply_error(error.into())));
        }
        let reload = match ReloadPlan::prepare(
            &validated.detected,
            &validated.change,
            Some(&request.expected_configuration_candidate_hash),
        ) {
            Ok(value) => value,
            Err(error) => return Ok(Response::new(apply_error(error))),
        };
        if let Err(error) = reload.preflight().await {
            return Ok(Response::new(apply_error(error)));
        }
        let applied = self.apply_transaction(&request, &validated.change).await?;
        let applied = match applied {
            Ok(value) => value,
            Err(error) => return Ok(Response::new(apply_error(error.into()))),
        };
        if let Err(reload_error) = reload.reload_applied().await {
            if applied.replayed {
                return Ok(Response::new(ApplyChangeResponse {
                    applied: true,
                    reloaded: false,
                    error: Some(error_detail(&reload_error)),
                    replayed: true,
                    rolled_back: false,
                    rollback_reloaded: false,
                }));
            }
            return Ok(Response::new(
                self.recover_failed_reload(
                    &request.change_id,
                    &validated.change,
                    &reload,
                    reload_error,
                    applied.replayed,
                )
                .await,
            ));
        }
        Ok(Response::new(ApplyChangeResponse {
            applied: applied.applied,
            reloaded: true,
            error: None,
            replayed: applied.replayed,
            rolled_back: false,
            rollback_reloaded: false,
        }))
    }

    pub(crate) async fn rollback_change_rpc(
        &self,
        request: RollbackChangeRequest,
    ) -> Result<Response<RollbackChangeResponse>, Status> {
        self.authenticate(request.context.as_ref(), Some(&request.operation_id))?;
        let _mutation = self.mutation_gate.lock().await;
        let reload = self
            .validate_prepared_client(&request.change_id)
            .await
            .and_then(|validated| {
                ReloadPlan::prepare(&validated.detected, &validated.change, None)
            });
        let transactions = self.transactions.clone();
        let change_id = request.change_id;
        let backup_id = request.backup_id;
        let rolled_back =
            tokio::task::spawn_blocking(move || transactions.rollback(&change_id, &backup_id))
                .await
                .map_err(|_| Status::internal("适配器事务任务失败"))?;
        let value = match rolled_back {
            Ok(value) => value,
            Err(error) => {
                return Ok(Response::new(RollbackChangeResponse {
                    restored: false,
                    reloaded: false,
                    error: Some(error_detail(&error.into())),
                    replayed: false,
                }));
            }
        };
        let reload = match reload {
            Ok(reload) => reload,
            Err(error) => {
                return Ok(Response::new(RollbackChangeResponse {
                    restored: value.restored,
                    reloaded: false,
                    error: Some(error_detail(&error)),
                    replayed: value.replayed,
                }));
            }
        };
        match reload.reload_restored().await {
            Ok(()) => Ok(Response::new(RollbackChangeResponse {
                restored: value.restored,
                reloaded: true,
                error: None,
                replayed: value.replayed,
            })),
            Err(error) => Ok(Response::new(RollbackChangeResponse {
                restored: value.restored,
                reloaded: false,
                error: Some(error_detail(&error)),
                replayed: value.replayed,
            })),
        }
    }

    async fn apply_transaction(
        &self,
        request: &ApplyChangeRequest,
        change: &ChangeInstallation,
    ) -> Result<Result<ApplyOutcome, nonproxy_adapter_transaction::AdapterTransactionError>, Status>
    {
        let transactions = self.transactions.clone();
        let change_id = request.change_id.clone();
        let expected_hash = request.expected_candidate_hash.clone();
        let expected_configuration_hash = request.expected_configuration_candidate_hash.clone();
        let integrated = change.main_configuration_path.is_some();
        let now =
            super::change_rpc::now_unix_millis().map_err(|_| Status::internal("系统时间无效"))?;
        tokio::task::spawn_blocking(move || {
            if integrated {
                transactions.apply_integrated(
                    &change_id,
                    &expected_hash,
                    &expected_configuration_hash,
                    now,
                )
            } else {
                transactions.apply(&change_id, &expected_hash, now)
            }
        })
        .await
        .map_err(|_| Status::internal("适配器事务任务失败"))
    }

    async fn preflight_transaction(
        &self,
        request: &ApplyChangeRequest,
        change: &ChangeInstallation,
    ) -> Result<Result<(), nonproxy_adapter_transaction::AdapterTransactionError>, Status> {
        let transactions = self.transactions.clone();
        let change_id = request.change_id.clone();
        let expected_hash = request.expected_candidate_hash.clone();
        let expected_configuration_hash = request.expected_configuration_candidate_hash.clone();
        let integrated = change.main_configuration_path.is_some();
        let now =
            super::change_rpc::now_unix_millis().map_err(|_| Status::internal("系统时间无效"))?;
        tokio::task::spawn_blocking(move || {
            if integrated {
                transactions.preflight_apply_integrated(
                    &change_id,
                    &expected_hash,
                    &expected_configuration_hash,
                    now,
                )
            } else {
                transactions.preflight_apply(&change_id, &expected_hash, now)
            }
        })
        .await
        .map_err(|_| Status::internal("适配器事务任务失败"))
    }

    async fn recover_failed_reload(
        &self,
        change_id: &str,
        change: &ChangeInstallation,
        reload: &ReloadPlan,
        reload_error: AdapterHostError,
        replayed: bool,
    ) -> ApplyChangeResponse {
        let transactions = self.transactions.clone();
        let change_id = change_id.to_owned();
        let backup_id = change.backup_id.clone();
        let rolled_back =
            tokio::task::spawn_blocking(move || transactions.rollback(&change_id, &backup_id))
                .await;
        let Ok(Ok(value)) = rolled_back else {
            return recovery_error(false, false, replayed);
        };
        if reload.reload_restored().await.is_err() {
            return recovery_error(value.restored, false, replayed);
        }
        ApplyChangeResponse {
            applied: false,
            reloaded: false,
            error: Some(error_detail(&reload_error)),
            replayed,
            rolled_back: value.restored,
            rollback_reloaded: true,
        }
    }

    async fn validate_prepared_client(
        &self,
        change_id: &str,
    ) -> Result<ValidatedChange, AdapterHostError> {
        let transactions = self.transactions.clone();
        let owned_change_id = change_id.to_owned();
        let expected =
            tokio::task::spawn_blocking(move || transactions.change_installation(&owned_change_id))
                .await
                .map_err(AdapterHostError::Task)??;
        let registered = self.catalog.get(&expected.adapter_id)?;
        if registered.client != expected.client
            || registered.managed_rules_path != expected.managed_rules_path
            || registered.main_configuration_path != expected.main_configuration_path
            || registered.direct_target != expected.requested_direct_target
        {
            return Err(AdapterHostError::InstallationChanged);
        }
        let detected = detect(registered.client, &registered.executable_path).await?;
        if detected.version != expected.client_version {
            return Err(AdapterHostError::ClientVersionChanged);
        }
        if !detected.supported() || !hot_reload_supported(detected.client, detected.version) {
            return Err(AdapterHostError::ClientUnsupported);
        }
        Ok(ValidatedChange {
            change: expected,
            detected,
        })
    }
}

fn apply_error(error: AdapterHostError) -> ApplyChangeResponse {
    ApplyChangeResponse {
        applied: false,
        reloaded: false,
        error: Some(error_detail(&error)),
        replayed: false,
        rolled_back: false,
        rollback_reloaded: false,
    }
}

fn recovery_error(rolled_back: bool, reloaded: bool, replayed: bool) -> ApplyChangeResponse {
    ApplyChangeResponse {
        applied: false,
        reloaded: false,
        error: Some(error_detail(&AdapterHostError::ClientRecoveryFailed)),
        replayed,
        rolled_back,
        rollback_reloaded: reloaded,
    }
}
