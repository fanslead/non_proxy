using NonProxy.Control.V1;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public interface IControlRpcClient
{
    Task<GetSystemStatusResponse> GetSystemStatusAsync(
        CancellationToken cancellationToken);

    Task<ListPoliciesResponse> ListPoliciesAsync(
        string pageToken,
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
        CancellationToken cancellationToken);

    Task<ListOutboundsResponse> ListOutboundsAsync(
        string pageToken,
        CancellationToken cancellationToken);

    Task<ImportConfigurationResponse> ImportConfigurationAsync(
        byte[] configuration,
        CancellationToken cancellationToken);

    Task<TestOutboundResponse> TestOutboundAsync(
        string outboundId,
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
}
