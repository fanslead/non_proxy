using System.Text;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Tests;

internal sealed class StubControlRpcClient : IControlRpcClient
{
    public Queue<ListPoliciesResponse> PoliciesResponses { get; } = new();

    public GetSystemStatusResponse StatusResponse { get; set; } = new();

    public ListPoliciesResponse PoliciesResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
    };

    public UpsertPolicyResponse UpsertResponse { get; set; } = new();

    public DeletePolicyResponse DeleteResponse { get; set; } = new();

    public ApplyPolicySnapshotResponse ApplyResponse { get; set; } = new();

    public ControlServiceException? ApplyException { get; set; }

    public RollbackPolicySnapshotResponse RollbackResponse { get; set; } = new();

    public ListOutboundsResponse OutboundsResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
    };

    public ImportConfigurationResponse ImportResponse { get; set; } = new();

    public StartLearningSessionResponse StartLearningResponse { get; set; } = new();

    public RecordLearningObservationResponse RecordLearningResponse { get; set; } = new();

    public ListLearningCandidatesResponse LearningCandidatesResponse { get; set; } = new();

    public StopLearningSessionResponse StopLearningResponse { get; set; } = new();

    public string? LastImportedConfiguration { get; private set; }

    public ProtoPolicy? LastUpsertedPolicy { get; private set; }

    public ulong LastExpectedRevision { get; private set; }

    public int ListPoliciesCallCount { get; private set; }

    public Task<GetSystemStatusResponse> GetSystemStatusAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(StatusResponse);
    }

    public Task<ListPoliciesResponse> ListPoliciesAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        ListPoliciesCallCount++;
        var response = PoliciesResponses.Count > 0
            ? PoliciesResponses.Dequeue()
            : PoliciesResponse;
        return Task.FromResult(response);
    }

    public Task<UpsertPolicyResponse> UpsertPolicyAsync(
        ProtoPolicy policy,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastUpsertedPolicy = policy;
        LastExpectedRevision = expectedRevision;
        return Task.FromResult(UpsertResponse);
    }

    public Task<DeletePolicyResponse> DeletePolicyAsync(
        string policyId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastExpectedRevision = expectedRevision;
        return Task.FromResult(DeleteResponse);
    }

    public Task<ApplyPolicySnapshotResponse> ApplySnapshotAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (ApplyException is not null)
        {
            throw ApplyException;
        }

        return Task.FromResult(ApplyResponse);
    }

    public Task<RollbackPolicySnapshotResponse> RollBackAsync(
        ulong snapshotVersion,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(RollbackResponse);
    }

    public Task<ListOutboundsResponse> ListOutboundsAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(OutboundsResponse);
    }

    public Task<ImportConfigurationResponse> ImportConfigurationAsync(
        byte[] configuration,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastImportedConfiguration = Encoding.UTF8.GetString(configuration);
        return Task.FromResult(ImportResponse);
    }

    public Task<StartLearningSessionResponse> StartLearningSessionAsync(
        StartLearningSessionRequest request,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(StartLearningResponse);
    }

    public Task<RecordLearningObservationResponse> RecordLearningObservationAsync(
        RecordLearningObservationRequest request,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(RecordLearningResponse);
    }

    public Task<ListLearningCandidatesResponse> ListLearningCandidatesAsync(
        string sessionId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(LearningCandidatesResponse);
    }

    public Task<StopLearningSessionResponse> StopLearningSessionAsync(
        string sessionId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(StopLearningResponse);
    }
}
