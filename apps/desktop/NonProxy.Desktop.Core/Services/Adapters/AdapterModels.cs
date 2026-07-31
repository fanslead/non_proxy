using NonProxy.Adapter.V1;
using NonProxy.Common.V1;

namespace NonProxy.Desktop.Core.Services.Adapters;

public sealed record AdapterInstallationItem(
    string Id,
    AdapterClient Client,
    string ClientName,
    string ClientVersion,
    string ExecutablePath,
    string ManagedRulesPath,
    string MainConfigurationPath,
    string? DirectTarget,
    AdapterState State)
{
    public string StateLabel => State switch
    {
        AdapterState.Available => "已登记",
        AdapterState.Ready => "配置已确认",
        AdapterState.Applying => "正在同步",
        AdapterState.Unsupported => "版本不受支持",
        AdapterState.NotInstalled => "客户端不存在",
        AdapterState.Failed => "需要处理",
        _ => "状态未知",
    };
}

public sealed record AdapterCatalog(
    IReadOnlyList<AdapterInstallationItem> Items,
    DateTimeOffset CapturedAt);

public sealed record AdapterRegistrationDraft(
    string Id,
    AdapterClient Client,
    string ExecutablePath,
    string ManagedRulesPath,
    string MainConfigurationPath,
    string? DirectTarget = null);

public sealed record AdapterMutationResult(
    bool Succeeded,
    string Code,
    string Message,
    AdapterInstallationItem? Installation = null);

public sealed record AdapterProjectionBlocker(
    string PolicyId,
    string Code,
    string Message);

public sealed record AdapterPolicyProjection(
    byte[] Payload,
    byte[] PayloadHash,
    int RuleCount,
    IReadOnlyList<AdapterProjectionBlocker> Blockers);

public sealed record AdapterSyncResult(
    bool Succeeded,
    string Code,
    string Message,
    ulong SnapshotVersion,
    int RuleCount,
    bool ClientValidated,
    bool Reloaded,
    bool ConfigurationVerified,
    bool PathVerified,
    EvidenceLevel EvidenceLevel,
    IReadOnlyList<AdapterProjectionBlocker> Blockers)
{
    public static AdapterSyncResult Rejected(
        string code,
        string message,
        ulong snapshotVersion = 0,
        IReadOnlyList<AdapterProjectionBlocker>? blockers = null)
    {
        return new AdapterSyncResult(
            false,
            code,
            message,
            snapshotVersion,
            0,
            false,
            false,
            false,
            false,
            EvidenceLevel.Unspecified,
            blockers ?? Array.Empty<AdapterProjectionBlocker>());
    }
}

public interface IAdapterManagementService
{
    Task<AdapterCatalog> ListAsync(CancellationToken cancellationToken);

    Task<AdapterMutationResult> RegisterAsync(
        AdapterRegistrationDraft draft,
        CancellationToken cancellationToken);

    Task<AdapterMutationResult> RemoveAsync(
        string adapterId,
        CancellationToken cancellationToken);

    Task<AdapterSyncResult> SyncAsync(
        string adapterId,
        CancellationToken cancellationToken);
}
