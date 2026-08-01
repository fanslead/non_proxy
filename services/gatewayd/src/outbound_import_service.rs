use std::sync::Arc;

use nonproxy_proto::{
    common::v1::ErrorDetail,
    control::v1::{ImportConfigurationRequest, ImportConfigurationResponse},
};
use zeroize::Zeroizing;

use crate::{
    Gateway, GatewayError,
    credential_store::{CredentialStore, delete_credentials, store_credentials},
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

    let new_references =
        match store_credentials(Arc::clone(&credential_store), prepared.credentials).await {
            Ok(value) => value,
            Err(failure) => {
                return credential_failure(
                    "代理凭据写入失败，出口配置没有改变。",
                    failure.cleanup_failures(),
                );
            }
        };
    if let Err(error) = gateway.save_outbounds(prepared.outbounds).await {
        let cleanup_failures =
            delete_credentials(Arc::clone(&credential_store), new_references).await;
        let mut response = gateway_failure(error);
        append_cleanup_warning(&mut response, cleanup_failures);
        return response;
    }

    let mut warnings = prepared.warnings;
    let stale = prepared
        .replaced_credential_references
        .into_iter()
        .filter(|reference| !new_references.contains(reference))
        .collect();
    let cleanup_failures = delete_credentials(credential_store, stale).await;
    append_warning(&mut warnings, cleanup_failures);
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

fn credential_failure(message: &str, cleanup_failures: usize) -> ImportConfigurationResponse {
    let mut response = response_error("NP_CREDENTIAL_STORE_FAILED", message, true);
    append_cleanup_warning(&mut response, cleanup_failures);
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

fn append_cleanup_warning(response: &mut ImportConfigurationResponse, cleanup_failures: usize) {
    append_warning(&mut response.warnings, cleanup_failures);
}

fn append_warning(warnings: &mut Vec<String>, cleanup_failures: usize) {
    if cleanup_failures > 0 {
        warnings.push("代理凭据未能全部清理，可在系统凭据库中手动删除未引用项。".to_owned());
    }
}
