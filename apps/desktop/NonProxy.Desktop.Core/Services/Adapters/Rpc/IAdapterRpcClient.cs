using NonProxy.Adapter.V1;

namespace NonProxy.Desktop.Core.Services.Adapters.Rpc;

public interface IAdapterRpcClient
{
    Task<ListInstallationsResponse> ListInstallationsAsync(
        CancellationToken cancellationToken);

    Task<RegisterInstallationResponse> RegisterInstallationAsync(
        string adapterId,
        AdapterClient client,
        string executablePath,
        string managedRulesPath,
        string mainConfigurationPath,
        string? directTarget,
        CancellationToken cancellationToken);

    Task<RemoveInstallationResponse> RemoveInstallationAsync(
        string adapterId,
        CancellationToken cancellationToken);

    Task<DetectResponse> DetectAsync(
        string adapterId,
        CancellationToken cancellationToken);

    Task<ReadCapabilitiesResponse> ReadCapabilitiesAsync(
        string adapterId,
        string installationId,
        CancellationToken cancellationToken);

    Task<PrepareChangeResponse> PrepareChangeAsync(
        string adapterId,
        string installationId,
        byte[] normalizedPolicy,
        byte[] normalizedPolicyHash,
        CancellationToken cancellationToken);

    Task<ApplyChangeResponse> ApplyChangeAsync(
        string changeId,
        byte[] candidateHash,
        byte[] configurationCandidateHash,
        CancellationToken cancellationToken);

    Task<VerifyChangeResponse> VerifyChangeAsync(
        string changeId,
        CancellationToken cancellationToken);

    Task<RollbackChangeResponse> RollbackChangeAsync(
        string changeId,
        string backupId,
        CancellationToken cancellationToken);
}
