using NonProxy.Control.V1;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public interface IControlRpcClient
{
    Task<GetSystemStatusResponse> GetSystemStatusAsync(
        CancellationToken cancellationToken);

    Task<GetRuntimeOverrideStatusResponse> GetRuntimeOverrideStatusAsync(
        CancellationToken cancellationToken);

    Task<SetRuntimeOverrideResponse> SetRuntimeOverrideAsync(
        NonProxy.Policy.V1.RuntimeOverrideMode mode,
        TimeSpan duration,
        string? outboundId,
        ulong expectedActiveSnapshotVersion,
        CancellationToken cancellationToken);

    Task<ClearRuntimeOverrideResponse> ClearRuntimeOverrideAsync(
        ulong expectedActiveSnapshotVersion,
        CancellationToken cancellationToken);

    Task<ListPoliciesResponse> ListPoliciesAsync(
        string pageToken,
        CancellationToken cancellationToken);

    Task<GetActivePolicySnapshotResponse> GetActivePolicySnapshotAsync(
        CancellationToken cancellationToken);

    Task<UpsertPolicyResponse> UpsertPolicyAsync(
        ProtoPolicy policy,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<DeletePolicyResponse> DeletePolicyAsync(
        string policyId,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<ApplyPolicySnapshotResponse> ApplySnapshotAsync(
        CancellationToken cancellationToken);

    Task<RollbackPolicySnapshotResponse> RollBackAsync(
        ulong snapshotVersion,
        ulong expectedActiveSnapshotVersion,
        CancellationToken cancellationToken);

    Task<ListOutboundsResponse> ListOutboundsAsync(
        string pageToken,
        CancellationToken cancellationToken);

    Task<ListOutboundGroupsResponse> ListOutboundGroupsAsync(
        string pageToken,
        CancellationToken cancellationToken);

    Task<UpsertOutboundGroupResponse> UpsertOutboundGroupAsync(
        string groupId,
        string displayName,
        IReadOnlyList<string> outboundIds,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<DeleteOutboundGroupResponse> DeleteOutboundGroupAsync(
        string groupId,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<ListSubscriptionSourcesResponse> ListSubscriptionSourcesAsync(
        string pageToken,
        CancellationToken cancellationToken);

    Task<UpsertSubscriptionSourceResponse> UpsertSubscriptionSourceAsync(
        string sourceId,
        string displayName,
        byte[] endpointUrl,
        bool enabled,
        TimeSpan refreshInterval,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<RefreshSubscriptionSourceResponse> RefreshSubscriptionSourceAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<DeleteSubscriptionSourceResponse> DeleteSubscriptionSourceAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<ListNetworkProfilesResponse> ListNetworkProfilesAsync(
        string pageToken,
        CancellationToken cancellationToken);

    Task<UpsertNetworkProfileResponse> UpsertNetworkProfileAsync(
        NonProxy.Policy.V1.NetworkProfileSpec profile,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<DeleteNetworkProfileResponse> DeleteNetworkProfileAsync(
        string profileId,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<ListConnectionDecisionsResponse> ListConnectionDecisionsAsync(
        int pageSize,
        string pageToken,
        CancellationToken cancellationToken);

    Task<ListExitProbesResponse> ListExitProbesAsync(
        int pageSize,
        string pageToken,
        CancellationToken cancellationToken);

    Task<ImportConfigurationResponse> ImportConfigurationAsync(
        string format,
        byte[] configuration,
        bool validateOnly,
        CancellationToken cancellationToken);

    Task<TestOutboundResponse> TestOutboundAsync(
        string outboundId,
        CancellationToken cancellationToken);

    Task<VerifyExitResponse> VerifyExitAsync(
        string? outboundId,
        CancellationToken cancellationToken);

    Task<SetDefaultRouteResponse> SetDefaultRouteAsync(
        string outboundId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken);

    Task<SetDefaultRouteResponse> SetDirectRouteAsync(
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken);

    Task<SetDefaultRouteResponse> SetDefaultOutboundGroupAsync(
        string groupId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken);

    Task<StartLearningSessionResponse> StartLearningSessionAsync(
        StartLearningSessionRequest request,
        CancellationToken cancellationToken);

    Task<RecordLearningObservationResponse> RecordLearningObservationAsync(
        RecordLearningObservationRequest request,
        CancellationToken cancellationToken);

    Task<ListLearningCandidatesResponse> ListLearningCandidatesAsync(
        string sessionId,
        CancellationToken cancellationToken);

    Task<StopLearningSessionResponse> StopLearningSessionAsync(
        string sessionId,
        CancellationToken cancellationToken);

    Task<ExportDiagnosticsResponse> ExportDiagnosticsAsync(
        CancellationToken cancellationToken);
}
