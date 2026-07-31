use nonproxy_proto::adapter::v1::{
    ApplyChangeRequest, ApplyChangeResponse, DetectRequest, DetectResponse,
    ListInstallationsRequest, ListInstallationsResponse, PrepareChangeRequest,
    PrepareChangeResponse, ReadCapabilitiesRequest, ReadCapabilitiesResponse,
    RegisterInstallationRequest, RegisterInstallationResponse, RemoveInstallationRequest,
    RemoveInstallationResponse, RollbackChangeRequest, RollbackChangeResponse, VerifyChangeRequest,
    VerifyChangeResponse, adapter_service_server::AdapterService,
};
use tonic::{Request, Response, Status};

use crate::rpc_state::AdapterRpcService;

#[tonic::async_trait]
impl AdapterService for AdapterRpcService {
    async fn register_installation(
        &self,
        request: Request<RegisterInstallationRequest>,
    ) -> Result<Response<RegisterInstallationResponse>, Status> {
        self.register_installation_rpc(request.into_inner()).await
    }

    async fn list_installations(
        &self,
        request: Request<ListInstallationsRequest>,
    ) -> Result<Response<ListInstallationsResponse>, Status> {
        self.list_installations_rpc(request.into_inner()).await
    }

    async fn remove_installation(
        &self,
        request: Request<RemoveInstallationRequest>,
    ) -> Result<Response<RemoveInstallationResponse>, Status> {
        self.remove_installation_rpc(request.into_inner()).await
    }

    async fn detect(
        &self,
        request: Request<DetectRequest>,
    ) -> Result<Response<DetectResponse>, Status> {
        self.detect_rpc(request.into_inner()).await
    }

    async fn read_capabilities(
        &self,
        request: Request<ReadCapabilitiesRequest>,
    ) -> Result<Response<ReadCapabilitiesResponse>, Status> {
        self.read_capabilities_rpc(request.into_inner()).await
    }

    async fn prepare_change(
        &self,
        request: Request<PrepareChangeRequest>,
    ) -> Result<Response<PrepareChangeResponse>, Status> {
        self.prepare_change_rpc(request.into_inner()).await
    }

    async fn apply_change(
        &self,
        request: Request<ApplyChangeRequest>,
    ) -> Result<Response<ApplyChangeResponse>, Status> {
        self.apply_change_rpc(request.into_inner()).await
    }

    async fn verify_change(
        &self,
        request: Request<VerifyChangeRequest>,
    ) -> Result<Response<VerifyChangeResponse>, Status> {
        self.verify_change_rpc(request.into_inner()).await
    }

    async fn rollback_change(
        &self,
        request: Request<RollbackChangeRequest>,
    ) -> Result<Response<RollbackChangeResponse>, Status> {
        self.rollback_change_rpc(request.into_inner()).await
    }
}
