use std::path::Path;

use nonproxy_proto::adapter::v1::{
    AdapterState, DetectRequest, DetectResponse, ListInstallationsRequest,
    ListInstallationsResponse, ReadCapabilitiesRequest, ReadCapabilitiesResponse,
    RegisterInstallationRequest, RegisterInstallationResponse, RemoveInstallationRequest,
    RemoveInstallationResponse,
};
use tonic::{Response, Status};

use crate::{
    AdapterHostError,
    capabilities::capabilities,
    catalog::validate_identifier,
    detection::detect,
    mapping::{
        detected_response, error_detail, installation_message, parse_client, registered_state,
    },
    model::RegisteredInstallation,
    path_validation::{
        canonical_main_configuration, canonical_managed_path, validate_integration_paths,
    },
    rpc_state::AdapterRpcService,
};

impl AdapterRpcService {
    pub(crate) async fn register_installation_rpc(
        &self,
        request: RegisterInstallationRequest,
    ) -> Result<Response<RegisterInstallationResponse>, Status> {
        self.authenticate(request.context.as_ref(), None)?;
        let _mutation = self.mutation_gate.lock().await;
        let result = self.register_installation(request).await;
        Ok(Response::new(match result {
            Ok((installation, replayed)) => RegisterInstallationResponse {
                installation: Some(installation_message(
                    &installation,
                    registered_state(&installation),
                )),
                replayed,
                error: None,
            },
            Err(error) => RegisterInstallationResponse {
                installation: None,
                replayed: false,
                error: Some(error_detail(&error)),
            },
        }))
    }

    pub(crate) async fn list_installations_rpc(
        &self,
        request: ListInstallationsRequest,
    ) -> Result<Response<ListInstallationsResponse>, Status> {
        self.authenticate(request.context.as_ref(), None)?;
        let catalog = self.catalog.clone();
        let listed = tokio::task::spawn_blocking(move || catalog.list())
            .await
            .map_err(|_| Status::internal("适配器目录任务失败"))?;
        Ok(Response::new(match listed {
            Ok(values) => ListInstallationsResponse {
                installations: values
                    .iter()
                    .map(|value| installation_message(value, registered_state(value)))
                    .collect(),
                error: None,
            },
            Err(error) => ListInstallationsResponse {
                installations: Vec::new(),
                error: Some(error_detail(&error)),
            },
        }))
    }

    pub(crate) async fn remove_installation_rpc(
        &self,
        request: RemoveInstallationRequest,
    ) -> Result<Response<RemoveInstallationResponse>, Status> {
        self.authenticate(request.context.as_ref(), None)?;
        let _mutation = self.mutation_gate.lock().await;
        let catalog = self.catalog.clone();
        let adapter_id = request.adapter_id;
        let removed = tokio::task::spawn_blocking(move || catalog.remove(&adapter_id))
            .await
            .map_err(|_| Status::internal("适配器目录任务失败"))?;
        Ok(Response::new(match removed {
            Ok(removed) => RemoveInstallationResponse {
                removed,
                error: None,
            },
            Err(error) => RemoveInstallationResponse {
                removed: false,
                error: Some(error_detail(&error)),
            },
        }))
    }

    pub(crate) async fn detect_rpc(
        &self,
        request: DetectRequest,
    ) -> Result<Response<DetectResponse>, Status> {
        self.authenticate(request.context.as_ref(), None)?;
        let installation = self.catalog.get(&request.adapter_id);
        let response = match installation {
            Ok(installation) => {
                match detect(installation.client, &installation.executable_path).await {
                    Ok(detected) => detected_response(&installation, &detected),
                    Err(error) => DetectResponse {
                        state: match &error {
                            AdapterHostError::InstallationInvalid => {
                                AdapterState::NotInstalled.into()
                            }
                            _ => AdapterState::Failed.into(),
                        },
                        client_name: crate::mapping::client_name(installation.client).to_owned(),
                        client_version: String::new(),
                        installation_id: installation.adapter_id,
                        error: Some(error_detail(&error)),
                    },
                }
            }
            Err(error) => DetectResponse {
                state: AdapterState::NotInstalled.into(),
                client_name: String::new(),
                client_version: String::new(),
                installation_id: String::new(),
                error: Some(error_detail(&error)),
            },
        };
        Ok(Response::new(response))
    }

    pub(crate) async fn read_capabilities_rpc(
        &self,
        request: ReadCapabilitiesRequest,
    ) -> Result<Response<ReadCapabilitiesResponse>, Status> {
        self.authenticate(request.context.as_ref(), None)?;
        let result = self.read_capabilities(request).await;
        Ok(Response::new(match result {
            Ok(values) => ReadCapabilitiesResponse {
                capabilities: values.into_iter().map(i32::from).collect(),
                error: None,
            },
            Err(error) => ReadCapabilitiesResponse {
                capabilities: Vec::new(),
                error: Some(error_detail(&error)),
            },
        }))
    }

    async fn register_installation(
        &self,
        request: RegisterInstallationRequest,
    ) -> Result<(RegisteredInstallation, bool), AdapterHostError> {
        validate_identifier(&request.adapter_id)?;
        let client = parse_client(request.client)?;
        let detected = detect(client, Path::new(&request.executable_path)).await?;
        let main_configuration_path =
            canonical_main_configuration(Path::new(&request.main_configuration_path))?;
        let managed_rules_path = canonical_managed_path(Path::new(&request.managed_rules_path))?;
        validate_integration_paths(&main_configuration_path, &managed_rules_path)?;
        let direct_target = (!request.direct_target.is_empty()).then_some(request.direct_target);
        let installation = RegisteredInstallation {
            adapter_id: request.adapter_id,
            client,
            client_version: detected.version,
            executable_path: detected.executable_path,
            managed_rules_path,
            main_configuration_path: Some(main_configuration_path),
            direct_target,
        };
        let catalog = self.catalog.clone();
        let stored = installation.clone();
        let replayed = tokio::task::spawn_blocking(move || catalog.register(stored))
            .await
            .map_err(AdapterHostError::Task)??;
        Ok((installation, replayed))
    }

    async fn read_capabilities(
        &self,
        request: ReadCapabilitiesRequest,
    ) -> Result<Vec<nonproxy_proto::adapter::v1::AdapterCapability>, AdapterHostError> {
        if !request.installation_id.is_empty() && request.installation_id != request.adapter_id {
            return Err(AdapterHostError::InstallationInvalid);
        }
        let installation = self.catalog.get(&request.adapter_id)?;
        let detected = detect(installation.client, &installation.executable_path).await?;
        Ok(capabilities(detected.client, detected.version))
    }
}
