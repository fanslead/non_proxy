use std::collections::HashMap;

use nonproxy_adapter_api::{AdapterClient as DomainClient, AdapterVersion};
use nonproxy_proto::{
    adapter::v1::{AdapterClient, AdapterInstallation, AdapterState, DetectResponse},
    common::v1::ErrorDetail,
};

use crate::{
    AdapterHostError, capabilities::capabilities, detection::DetectedClient,
    model::RegisteredInstallation,
};

pub(crate) fn parse_client(value: i32) -> Result<DomainClient, AdapterHostError> {
    match AdapterClient::try_from(value).unwrap_or(AdapterClient::Unspecified) {
        AdapterClient::Surge => Ok(DomainClient::Surge),
        AdapterClient::Mihomo => Ok(DomainClient::Mihomo),
        AdapterClient::SingBox => Ok(DomainClient::SingBox),
        AdapterClient::Unspecified => Err(AdapterHostError::InstallationInvalid),
    }
}

pub(crate) const fn proto_client(value: DomainClient) -> AdapterClient {
    match value {
        DomainClient::Surge => AdapterClient::Surge,
        DomainClient::Mihomo => AdapterClient::Mihomo,
        DomainClient::SingBox => AdapterClient::SingBox,
    }
}

pub(crate) const fn client_name(value: DomainClient) -> &'static str {
    match value {
        DomainClient::Surge => "Surge Mac",
        DomainClient::Mihomo => "Mihomo",
        DomainClient::SingBox => "sing-box",
    }
}

pub(crate) fn format_version(value: AdapterVersion) -> String {
    format!("{}.{}.{}", value.major, value.minor, value.patch)
}

pub(crate) fn installation_message(
    value: &RegisteredInstallation,
    state: AdapterState,
) -> AdapterInstallation {
    AdapterInstallation {
        adapter_id: value.adapter_id.clone(),
        client: proto_client(value.client).into(),
        client_name: client_name(value.client).to_owned(),
        client_version: format_version(value.client_version),
        executable_path: value.executable_path.to_string_lossy().into_owned(),
        managed_rules_path: value.managed_rules_path.to_string_lossy().into_owned(),
        state: state.into(),
        main_configuration_path: value
            .main_configuration_path
            .as_deref()
            .map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
        direct_target: value.direct_target.clone().unwrap_or_default(),
    }
}

pub(crate) fn registered_state(value: &RegisteredInstallation) -> AdapterState {
    if value.main_configuration_path.is_none() {
        AdapterState::Failed
    } else if capabilities(value.client, value.client_version).is_empty() {
        AdapterState::Unsupported
    } else {
        AdapterState::Available
    }
}

pub(crate) fn detected_response(
    installation: &RegisteredInstallation,
    detected: &DetectedClient,
) -> DetectResponse {
    let state = if detected.supported() {
        AdapterState::Available
    } else {
        AdapterState::Unsupported
    };
    DetectResponse {
        state: state.into(),
        client_name: client_name(detected.client).to_owned(),
        client_version: format_version(detected.version),
        installation_id: installation.adapter_id.clone(),
        error: None,
    }
}

pub(crate) fn error_detail(error: &AdapterHostError) -> ErrorDetail {
    ErrorDetail {
        code: error.code().to_owned(),
        message: error.to_string(),
        retryable: error.retryable(),
        metadata: HashMap::new(),
    }
}
