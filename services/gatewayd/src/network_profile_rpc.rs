use nonproxy_model::NetworkProfileId;
use nonproxy_proto::{
    common::v1::PageRequest,
    control::v1::{
        DeleteNetworkProfileRequest, DeleteNetworkProfileResponse, ListNetworkProfilesRequest,
        ListNetworkProfilesResponse, NetworkProfileMutationResult, UpsertNetworkProfileRequest,
        UpsertNetworkProfileResponse,
    },
    policy::v1::{NetworkFingerprintKind as ProtoFingerprintKind, NetworkProfileSpec},
};
use nonproxy_storage::{NetworkFingerprint, NetworkFingerprintKind, NetworkProfileReference};
use tonic::Status;

use crate::{Gateway, GatewayError, control_mapping};

pub async fn list(
    gateway: &Gateway,
    request: ListNetworkProfilesRequest,
) -> Result<ListNetworkProfilesResponse, Status> {
    let (profiles, catalog_generation) = gateway
        .list_network_profiles()
        .await
        .map_err(super::control_rpc_helpers::internal_status)?;
    let page = request.page.unwrap_or(PageRequest {
        page_size: 0,
        page_token: String::new(),
    });
    let (start, end, page) =
        control_mapping::page_bounds(page.page_size, &page.page_token, profiles.len())?;
    Ok(ListNetworkProfilesResponse {
        profiles: profiles[start..end].iter().map(to_proto).collect(),
        page: Some(page),
        catalog_generation,
    })
}

pub async fn upsert(
    gateway: &Gateway,
    request: UpsertNetworkProfileRequest,
) -> UpsertNetworkProfileResponse {
    let result = match request
        .profile
        .ok_or(GatewayError::InvalidContract("缺少 network profile"))
    {
        Ok(profile) => match from_proto(profile) {
            Ok(profile) => gateway
                .save_network_profile(
                    profile,
                    (request.expected_revision > 0).then_some(request.expected_revision),
                )
                .await
                .map(|profile| NetworkProfileMutationResult {
                    profile: Some(to_proto(&profile)),
                    error: None,
                })
                .unwrap_or_else(|error| mutation_error(&error)),
            Err(error) => mutation_error(&error),
        },
        Err(error) => mutation_error(&error),
    };
    UpsertNetworkProfileResponse {
        result: Some(result),
    }
}

pub async fn delete(
    gateway: &Gateway,
    request: DeleteNetworkProfileRequest,
) -> Result<DeleteNetworkProfileResponse, Status> {
    if request.expected_revision == 0 {
        return Err(Status::invalid_argument(
            "删除网络配置档必须提供 expected_revision",
        ));
    }
    let profile_id = NetworkProfileId::new(request.profile_id)
        .map_err(GatewayError::from)
        .map_err(super::control_rpc_helpers::request_status)?;
    let result = match gateway
        .delete_network_profile(profile_id, request.expected_revision)
        .await
    {
        Ok(()) => NetworkProfileMutationResult {
            profile: None,
            error: None,
        },
        Err(error) => mutation_error(&error),
    };
    Ok(DeleteNetworkProfileResponse {
        result: Some(result),
    })
}

pub fn to_proto(value: &NetworkProfileReference) -> NetworkProfileSpec {
    NetworkProfileSpec {
        id: value.id().as_str().to_owned(),
        display_name: value.display_name().to_owned(),
        fingerprint_kind: match value.fingerprint().kind() {
            NetworkFingerprintKind::WifiSsidSha256 => ProtoFingerprintKind::WifiSsidSha256,
            NetworkFingerprintKind::DefaultGatewaySha256 => {
                ProtoFingerprintKind::DefaultGatewaySha256
            }
            NetworkFingerprintKind::InterfaceClass => ProtoFingerprintKind::InterfaceClass,
        } as i32,
        fingerprint_value: value.fingerprint().value().to_owned(),
        revision: value.revision(),
    }
}

fn from_proto(value: NetworkProfileSpec) -> Result<NetworkProfileReference, GatewayError> {
    let kind = match ProtoFingerprintKind::try_from(value.fingerprint_kind) {
        Ok(ProtoFingerprintKind::WifiSsidSha256) => NetworkFingerprintKind::WifiSsidSha256,
        Ok(ProtoFingerprintKind::DefaultGatewaySha256) => {
            NetworkFingerprintKind::DefaultGatewaySha256
        }
        Ok(ProtoFingerprintKind::InterfaceClass) => NetworkFingerprintKind::InterfaceClass,
        Ok(ProtoFingerprintKind::Unspecified) | Err(_) => {
            return Err(GatewayError::InvalidContract("网络配置档指纹类型无效"));
        }
    };
    Ok(NetworkProfileReference::new(
        NetworkProfileId::new(value.id)?,
        value.display_name,
        NetworkFingerprint::new(kind, value.fingerprint_value)?,
        value.revision,
    )?)
}

fn mutation_error(error: &GatewayError) -> NetworkProfileMutationResult {
    NetworkProfileMutationResult {
        profile: None,
        error: Some(control_mapping::error_detail(error)),
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_proto::policy::v1::{NetworkFingerprintKind, NetworkProfileSpec};

    use super::from_proto;

    #[test]
    fn raw_wifi_name_is_rejected_at_the_rpc_boundary() {
        let result = from_proto(NetworkProfileSpec {
            id: "office".to_owned(),
            display_name: "办公室".to_owned(),
            fingerprint_kind: NetworkFingerprintKind::WifiSsidSha256 as i32,
            fingerprint_value: "Office WiFi".to_owned(),
            revision: 1,
        });

        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("原始 Wi-Fi 名称不得被接收为网络指纹"),
        };

        assert_eq!(error.code(), "NP_NETWORK_PROFILE_INVALID");
    }

    #[test]
    fn unspecified_fingerprint_kind_is_rejected() {
        let result = from_proto(NetworkProfileSpec {
            id: "office".to_owned(),
            display_name: "办公室".to_owned(),
            fingerprint_kind: NetworkFingerprintKind::Unspecified as i32,
            fingerprint_value: "a".repeat(64),
            revision: 1,
        });

        assert!(result.is_err());
    }
}
