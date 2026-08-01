using System.Text;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Policy.V1;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Tests;

internal sealed class StubControlRpcClient : IControlRpcClient
{
    public Queue<ListPoliciesResponse> PoliciesResponses { get; } = new();

    public Queue<ListNetworkProfilesResponse> NetworkProfilesResponses { get; } = new();

    public GetSystemStatusResponse StatusResponse { get; set; } = new();

    public GetRuntimeOverrideStatusResponse RuntimeOverrideStatusResponse { get; set; } = new();

    public SetRuntimeOverrideResponse SetRuntimeOverrideResponse { get; set; } = new();

    public ClearRuntimeOverrideResponse ClearRuntimeOverrideResponse { get; set; } = new();

    public ListPoliciesResponse PoliciesResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
    };

    public GetActivePolicySnapshotResponse ActivePolicySnapshotResponse { get; set; } = new();

    public Queue<GetActivePolicySnapshotResponse> ActivePolicySnapshotResponses { get; } = new();

    public UpsertPolicyResponse UpsertResponse { get; set; } = new();

    public DeletePolicyResponse DeleteResponse { get; set; } = new();

    public ApplyPolicySnapshotResponse ApplyResponse { get; set; } = new();

    public ControlServiceException? ApplyException { get; set; }

    public RollbackPolicySnapshotResponse RollbackResponse { get; set; } = new();

    public ListNetworkProfilesResponse NetworkProfilesResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
    };

    public UpsertNetworkProfileResponse UpsertNetworkProfileResponse { get; set; } = new();

    public DeleteNetworkProfileResponse DeleteNetworkProfileResponse { get; set; } = new();

    public ListOutboundsResponse OutboundsResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
        RoutingRevision = 1,
    };

    public Queue<ListOutboundsResponse> OutboundsResponses { get; } = new();

    public ListConnectionDecisionsResponse DecisionsResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
    };

    public int LastDecisionPageSize { get; private set; }

    public ListExitProbesResponse ExitProbesResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
    };

    public Queue<ListExitProbesResponse> ExitProbesResponses { get; } = new();

    public ImportConfigurationResponse ImportResponse { get; set; } = new();

    public TestOutboundResponse TestOutboundResponse { get; set; } = new();

    public VerifyExitResponse VerifyExitResponse { get; set; } = new();

    public SetDefaultRouteResponse SetDefaultRouteResponse { get; set; } = new();

    public StartLearningSessionResponse StartLearningResponse { get; set; } = new();

    public RecordLearningObservationResponse RecordLearningResponse { get; set; } = new();

    public ListLearningCandidatesResponse LearningCandidatesResponse { get; set; } = new();

    public StopLearningSessionResponse StopLearningResponse { get; set; } = new();

    public ExportDiagnosticsResponse ExportDiagnosticsResponse { get; set; } = new();

    public string? LastImportedConfiguration { get; private set; }

    public string? LastImportFormat { get; private set; }

    public bool LastImportWasValidationOnly { get; private set; }

    public string? LastTestedOutboundId { get; private set; }

    public string? LastVerifiedOutboundId { get; private set; }

    public bool LastExitRouteWasDirect { get; private set; }

    public string? LastDefaultOutboundId { get; private set; }

    public ulong LastExpectedRoutingRevision { get; private set; }

    public bool LastRouteWasDirect { get; private set; }

    public ProtoPolicy? LastUpsertedPolicy { get; private set; }

    public ulong LastRollbackSnapshotVersion { get; private set; }

    public ulong LastExpectedActiveSnapshotVersion { get; private set; }

    public RuntimeOverrideMode LastRuntimeOverrideMode { get; private set; }

    public TimeSpan LastRuntimeOverrideDuration { get; private set; }

    public string? LastRuntimeOverrideOutboundId { get; private set; }

    public NetworkProfileSpec? LastUpsertedNetworkProfile { get; private set; }

    public string? LastDeletedNetworkProfileId { get; private set; }

    public ulong LastExpectedRevision { get; private set; }

    public int ListPoliciesCallCount { get; private set; }

    public int ListNetworkProfilesCallCount { get; private set; }

    public Task<GetSystemStatusResponse> GetSystemStatusAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(StatusResponse);
    }

    public Task<GetRuntimeOverrideStatusResponse> GetRuntimeOverrideStatusAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(RuntimeOverrideStatusResponse);
    }

    public Task<SetRuntimeOverrideResponse> SetRuntimeOverrideAsync(
        RuntimeOverrideMode mode,
        TimeSpan duration,
        string? outboundId,
        ulong expectedActiveSnapshotVersion,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastRuntimeOverrideMode = mode;
        LastRuntimeOverrideDuration = duration;
        LastRuntimeOverrideOutboundId = outboundId;
        LastExpectedActiveSnapshotVersion = expectedActiveSnapshotVersion;
        return Task.FromResult(SetRuntimeOverrideResponse);
    }

    public Task<ClearRuntimeOverrideResponse> ClearRuntimeOverrideAsync(
        ulong expectedActiveSnapshotVersion,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastExpectedActiveSnapshotVersion = expectedActiveSnapshotVersion;
        return Task.FromResult(ClearRuntimeOverrideResponse);
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

    public Task<GetActivePolicySnapshotResponse> GetActivePolicySnapshotAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ActivePolicySnapshotResponses.Count > 0
            ? ActivePolicySnapshotResponses.Dequeue()
            : ActivePolicySnapshotResponse);
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
        ulong expectedActiveSnapshotVersion,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastRollbackSnapshotVersion = snapshotVersion;
        LastExpectedActiveSnapshotVersion = expectedActiveSnapshotVersion;
        return Task.FromResult(RollbackResponse);
    }

    public Task<ListOutboundsResponse> ListOutboundsAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(OutboundsResponses.Count > 0
            ? OutboundsResponses.Dequeue()
            : OutboundsResponse);
    }

    public Task<ListNetworkProfilesResponse> ListNetworkProfilesAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        ListNetworkProfilesCallCount++;
        return Task.FromResult(NetworkProfilesResponses.Count > 0
            ? NetworkProfilesResponses.Dequeue()
            : NetworkProfilesResponse);
    }

    public Task<UpsertNetworkProfileResponse> UpsertNetworkProfileAsync(
        NetworkProfileSpec profile,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastUpsertedNetworkProfile = profile;
        LastExpectedRevision = expectedRevision;
        return Task.FromResult(UpsertNetworkProfileResponse);
    }

    public Task<DeleteNetworkProfileResponse> DeleteNetworkProfileAsync(
        string profileId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastDeletedNetworkProfileId = profileId;
        LastExpectedRevision = expectedRevision;
        return Task.FromResult(DeleteNetworkProfileResponse);
    }

    public Task<ListConnectionDecisionsResponse> ListConnectionDecisionsAsync(
        int pageSize,
        string pageToken,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastDecisionPageSize = pageSize;
        return Task.FromResult(DecisionsResponse);
    }

    public Task<ListExitProbesResponse> ListExitProbesAsync(
        int pageSize,
        string pageToken,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ExitProbesResponses.Count > 0
            ? ExitProbesResponses.Dequeue()
            : ExitProbesResponse);
    }

    public Task<ImportConfigurationResponse> ImportConfigurationAsync(
        string format,
        byte[] configuration,
        bool validateOnly,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastImportFormat = format;
        LastImportWasValidationOnly = validateOnly;
        LastImportedConfiguration = Encoding.UTF8.GetString(configuration);
        return Task.FromResult(ImportResponse);
    }

    public Task<TestOutboundResponse> TestOutboundAsync(
        string outboundId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastTestedOutboundId = outboundId;
        return Task.FromResult(TestOutboundResponse);
    }

    public Task<VerifyExitResponse> VerifyExitAsync(
        string? outboundId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastVerifiedOutboundId = outboundId;
        LastExitRouteWasDirect = string.IsNullOrWhiteSpace(outboundId);
        return Task.FromResult(VerifyExitResponse);
    }

    public Task<SetDefaultRouteResponse> SetDefaultRouteAsync(
        string outboundId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastDefaultOutboundId = outboundId;
        LastExpectedRoutingRevision = expectedRoutingRevision;
        LastRouteWasDirect = false;
        return Task.FromResult(SetDefaultRouteResponse);
    }

    public Task<SetDefaultRouteResponse> SetDirectRouteAsync(
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastDefaultOutboundId = null;
        LastExpectedRoutingRevision = expectedRoutingRevision;
        LastRouteWasDirect = true;
        return Task.FromResult(SetDefaultRouteResponse);
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

    public Task<ExportDiagnosticsResponse> ExportDiagnosticsAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ExportDiagnosticsResponse);
    }
}
