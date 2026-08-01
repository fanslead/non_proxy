use std::sync::Arc;

use nonproxy_proto::{
    common::v1::ErrorDetail,
    control::v1::{ImportConfigurationRequest, ImportConfigurationResponse},
};
use zeroize::Zeroizing;

use crate::{
    Gateway, GatewayError,
    clock::unix_time_ms,
    credential_cleanup_service::{
        CredentialCleanupOutcome, cleanup_queued_references, queue_and_cleanup_references,
        store_credentials_with_cleanup,
    },
    credential_store::CredentialStore,
    outbound_import::{OutboundImportError, prepare},
};

pub async fn import(
    gateway: &Gateway,
    credential_store: Arc<dyn CredentialStore>,
    request: ImportConfigurationRequest,
) -> ImportConfigurationResponse {
    let configuration = Zeroizing::new(request.configuration);
    let current = match gateway.list_outbounds().await {
        Ok(value) => value,
        Err(error) => return gateway_failure(error),
    };
    let import_id = match new_import_id() {
        Ok(value) => value,
        Err(error) => return gateway_failure(error),
    };
    let prepared = match prepare(
        &request.format,
        configuration.as_slice(),
        import_id,
        &current,
    ) {
        Ok(value) => value,
        Err(error) => return import_failure(&error),
    };
    let summaries = prepared
        .outbounds
        .iter()
        .map(|(value, _)| crate::control_mapping::outbound_summary(value, None, false))
        .collect();
    if request.validate_only {
        return ImportConfigurationResponse {
            import_id: prepared.import_id,
            outbounds: summaries,
            warnings: prepared.warnings,
            error: None,
        };
    }

    let now_unix_ms = match unix_time_ms() {
        Ok(value) => value,
        Err(error) => return gateway_failure(error),
    };

    let new_references = match store_credentials_with_cleanup(
        gateway,
        Arc::clone(&credential_store),
        prepared.credentials,
        now_unix_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(cleanup) => {
            return credential_failure("代理凭据写入失败，出口配置没有改变。", cleanup);
        }
    };
    let stale = prepared
        .replaced_credential_references
        .into_iter()
        .filter(|reference| !new_references.contains(reference))
        .collect::<Vec<_>>();
    if let Err(error) = gateway
        .save_imported_outbounds(prepared.outbounds, stale.clone(), now_unix_ms)
        .await
    {
        let cleanup_failures = queue_and_cleanup_references(
            gateway,
            Arc::clone(&credential_store),
            new_references,
            now_unix_ms,
        )
        .await;
        let mut response = gateway_failure(error);
        append_cleanup_warning(&mut response, cleanup_failures);
        return response;
    }

    let mut warnings = prepared.warnings;
    let cleanup_failures = cleanup_queued_references(
        gateway,
        credential_store,
        stale.into_iter().map(|reference| (reference, 0)).collect(),
        now_unix_ms,
    )
    .await;
    append_warning(&mut warnings, cleanup_failures, true);
    ImportConfigurationResponse {
        import_id: prepared.import_id,
        outbounds: summaries,
        warnings,
        error: None,
    }
}

fn new_import_id() -> Result<String, GatewayError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| GatewayError::Random(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn import_failure(error: &OutboundImportError) -> ImportConfigurationResponse {
    let mut response = response_error(error.code(), &error.to_string(), false);
    if let (Some(line), Some(detail)) = (error.line(), response.error.as_mut()) {
        detail.metadata.insert("line".to_owned(), line.to_string());
    }
    response
}

fn credential_failure(
    message: &str,
    cleanup: CredentialCleanupOutcome,
) -> ImportConfigurationResponse {
    let mut response = response_error("NP_CREDENTIAL_STORE_FAILED", message, true);
    append_cleanup_warning(&mut response, cleanup);
    response
}

fn gateway_failure(error: GatewayError) -> ImportConfigurationResponse {
    response_error(error.code(), &error.to_string(), error.retryable())
}

fn response_error(code: &str, message: &str, retryable: bool) -> ImportConfigurationResponse {
    ImportConfigurationResponse {
        import_id: String::new(),
        outbounds: Vec::new(),
        warnings: Vec::new(),
        error: Some(ErrorDetail {
            code: code.to_owned(),
            message: message.to_owned(),
            retryable,
            metadata: Default::default(),
        }),
    }
}

fn append_cleanup_warning(
    response: &mut ImportConfigurationResponse,
    cleanup: CredentialCleanupOutcome,
) {
    append_warning(
        &mut response.warnings,
        cleanup.failure_count(),
        cleanup.retry_persisted(),
    );
}

fn append_warning(
    warnings: &mut Vec<String>,
    cleanup_failures: usize,
    cleanup_retry_persisted: bool,
) {
    if cleanup_failures > 0 {
        warnings.push(if cleanup_retry_persisted {
            "部分旧代理凭据正在等待后台重试清理。".to_owned()
        } else {
            "部分代理凭据未能清理，后台重试队列也未能写入。".to_owned()
        });
    }
}
