using NonProxy.Adapter.V1;
using NonProxy.Desktop.Core.Services.Adapters.Rpc;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

internal sealed class StubAdapterRpcClient : IAdapterRpcClient
{
    public ListInstallationsResponse ListResponse { get; set; } = new();

    public RegisterInstallationResponse RegisterResponse { get; set; } = new();

    public RemoveInstallationResponse RemoveResponse { get; set; } = new();

    public DetectResponse DetectResponse { get; set; } = new()
    {
        State = AdapterState.Available,
        InstallationId = "surge-main",
    };

    public ReadCapabilitiesResponse CapabilitiesResponse { get; set; } = new();

    public PrepareChangeResponse PrepareResponse { get; set; } = new();

    public ApplyChangeResponse ApplyResponse { get; set; } = new();

    public ControlServiceException? ApplyException { get; set; }

    public VerifyChangeResponse VerifyResponse { get; set; } = new();

    public ControlServiceException? VerifyException { get; set; }

    public RollbackChangeResponse RollbackResponse { get; set; } = new();

    public int PrepareCallCount { get; private set; }

    public int ApplyCallCount { get; private set; }

    public int RollbackCallCount { get; private set; }

    public byte[]? LastNormalizedPolicy { get; private set; }

    public byte[]? LastNormalizedPolicyHash { get; private set; }

    public Task<ListInstallationsResponse> ListInstallationsAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ListResponse);
    }

    public Task<RegisterInstallationResponse> RegisterInstallationAsync(
        string adapterId,
        AdapterClient client,
        string executablePath,
        string managedRulesPath,
        string mainConfigurationPath,
        string? directTarget,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(RegisterResponse);
    }

    public Task<RemoveInstallationResponse> RemoveInstallationAsync(
        string adapterId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(RemoveResponse);
    }

    public Task<DetectResponse> DetectAsync(
        string adapterId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(DetectResponse);
    }

    public Task<ReadCapabilitiesResponse> ReadCapabilitiesAsync(
        string adapterId,
        string installationId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(CapabilitiesResponse);
    }

    public Task<PrepareChangeResponse> PrepareChangeAsync(
        string adapterId,
        string installationId,
        byte[] normalizedPolicy,
        byte[] normalizedPolicyHash,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        PrepareCallCount++;
        LastNormalizedPolicy = normalizedPolicy.ToArray();
        LastNormalizedPolicyHash = normalizedPolicyHash.ToArray();
        return Task.FromResult(PrepareResponse);
    }

    public Task<ApplyChangeResponse> ApplyChangeAsync(
        string changeId,
        byte[] candidateHash,
        byte[] configurationCandidateHash,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        ApplyCallCount++;
        if (ApplyException is not null)
        {
            throw ApplyException;
        }
        return Task.FromResult(ApplyResponse);
    }

    public Task<VerifyChangeResponse> VerifyChangeAsync(
        string changeId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (VerifyException is not null)
        {
            throw VerifyException;
        }
        return Task.FromResult(VerifyResponse);
    }

    public Task<RollbackChangeResponse> RollbackChangeAsync(
        string changeId,
        string backupId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        RollbackCallCount++;
        return Task.FromResult(RollbackResponse);
    }
}
