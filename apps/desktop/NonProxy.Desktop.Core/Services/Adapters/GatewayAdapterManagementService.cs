using System.Security.Cryptography;
using NonProxy.Adapter.V1;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters.Rpc;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Core.Services.Adapters;

public sealed class GatewayAdapterManagementService(
    IAdapterRpcClient adapterClient,
    IControlRpcClient controlClient,
    IApplicationCatalog applicationCatalog,
    AdapterPolicyProjector projector) : IAdapterManagementService
{
    public async Task<AdapterCatalog> ListAsync(
        CancellationToken cancellationToken)
    {
        var response = await adapterClient.ListInstallationsAsync(
            cancellationToken);
        ThrowIfError(response.Error, "无法读取第三方客户端登记目录。");
        return new AdapterCatalog(
            response.Installations.Select(Map).ToArray(),
            DateTimeOffset.UtcNow);
    }

    public async Task<AdapterMutationResult> RegisterAsync(
        AdapterRegistrationDraft draft,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(draft);
        ValidateRegistration(draft);
        var response = await adapterClient.RegisterInstallationAsync(
            draft.Id.Trim(),
            draft.Client,
            draft.ExecutablePath,
            draft.ManagedRulesPath,
            draft.MainConfigurationPath,
            draft.DirectTarget,
            cancellationToken);
        if (response.Error is { } error)
        {
            return Rejected(error, "第三方客户端没有登记。");
        }
        if (response.Installation is null)
        {
            throw InvalidContract("适配器后台没有返回登记结果。");
        }

        var installation = Map(response.Installation);
        return new AdapterMutationResult(
            true,
            response.Replayed
                ? "NP_ADAPTER_INSTALLATION_UNCHANGED"
                : "NP_ADAPTER_INSTALLATION_REGISTERED",
            response.Replayed
                ? "该客户端已经按相同路径登记，无需重复修改。"
                : "第三方客户端已登记；同步前仍会重新检测版本和能力。",
            installation);
    }

    public async Task<AdapterMutationResult> RemoveAsync(
        string adapterId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(adapterId);
        var response = await adapterClient.RemoveInstallationAsync(
            adapterId,
            cancellationToken);
        if (response.Error is { } error)
        {
            return Rejected(error, "第三方客户端登记没有移除。");
        }
        return new AdapterMutationResult(
            true,
            response.Removed
                ? "NP_ADAPTER_INSTALLATION_REMOVED"
                : "NP_ADAPTER_INSTALLATION_ABSENT",
            response.Removed
                ? "登记已移除；第三方客户端配置和恢复材料没有被删除。"
                : "该登记已经不存在；没有改动第三方客户端文件。");
    }

    public async Task<AdapterSyncResult> SyncAsync(
        string adapterId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(adapterId);
        var detection = await adapterClient.DetectAsync(
            adapterId,
            cancellationToken);
        if (detection.Error is { } detectionError)
        {
            return RejectedSync(detectionError, "客户端检测失败。");
        }
        if (detection.State != AdapterState.Available
            || string.IsNullOrWhiteSpace(detection.InstallationId))
        {
            return AdapterSyncResult.Rejected(
                "NP_ADAPTER_CLIENT_UNAVAILABLE",
                detection.State == AdapterState.Unsupported
                    ? "当前第三方客户端版本不受支持。"
                    : "没有找到可同步的第三方客户端安装项。");
        }

        var capabilityResponse = await adapterClient.ReadCapabilitiesAsync(
            adapterId,
            detection.InstallationId,
            cancellationToken);
        if (capabilityResponse.Error is { } capabilityError)
        {
            return RejectedSync(capabilityError, "无法确认客户端能力。");
        }
        var capabilities = capabilityResponse.Capabilities
            .Select(value => (AdapterCapability)value)
            .Where(value => value != AdapterCapability.Unspecified)
            .ToHashSet();
        if (!capabilities.Contains(AdapterCapability.HotReload))
        {
            return AdapterSyncResult.Rejected(
                "NP_ADAPTER_HOT_RELOAD_REQUIRED",
                "当前客户端没有可安全使用的公开重载入口，未修改配置。");
        }

        var snapshot = await controlClient.GetActivePolicySnapshotAsync(
            cancellationToken);
        ValidateSnapshot(snapshot);
        var applications = RequiresApplicationCatalog(snapshot)
            ? await applicationCatalog.ListAsync(cancellationToken)
            : new ApplicationCatalogSnapshot(
                Array.Empty<ApplicationCatalogEntry>(),
                true,
                false,
                "当前活动快照不需要应用路径。");
        var projection = projector.Project(snapshot, applications, capabilities);
        if (projection.Blockers.Count > 0)
        {
            return AdapterSyncResult.Rejected(
                "NP_ADAPTER_PROJECTION_INCOMPLETE",
                $"有 {projection.Blockers.Count} 条生效直连规则无法无损同步，未修改第三方客户端。",
                snapshot.SnapshotVersion,
                projection.Blockers);
        }

        var prepared = await adapterClient.PrepareChangeAsync(
            adapterId,
            detection.InstallationId,
            projection.Payload,
            projection.PayloadHash,
            cancellationToken);
        if (prepared.Error is { } prepareError)
        {
            return RejectedSync(
                prepareError,
                "候选配置没有通过客户端原生校验。",
                snapshot.SnapshotVersion);
        }
        ValidatePrepared(prepared, projection.RuleCount);

        var currentSnapshot = await controlClient.GetActivePolicySnapshotAsync(
            cancellationToken);
        ValidateSnapshot(currentSnapshot);
        if (!SameSnapshot(snapshot, currentSnapshot))
        {
            return AdapterSyncResult.Rejected(
                "NP_ADAPTER_SNAPSHOT_CHANGED",
                "准备候选期间生效策略已经变化；候选没有写入客户端，请重新同步。",
                currentSnapshot.SnapshotVersion);
        }

        ApplyChangeResponse applied;
        try
        {
            applied = await adapterClient.ApplyChangeAsync(
                prepared.ChangeId,
                prepared.CandidateHash.ToByteArray(),
                prepared.ConfigurationCandidateHash.ToByteArray(),
                cancellationToken);
        }
        catch (ControlServiceException exception)
        {
            return await RecoverPreparedChangeAsync(
                prepared,
                exception.Code,
                $"客户端应用状态无法确认：{exception.UserMessage}",
                snapshot.SnapshotVersion,
                cancellationToken);
        }
        if (applied.Error is { } applyError)
        {
            if (applied.RolledBack && applied.RollbackReloaded)
            {
                return RejectedSync(
                    applyError,
                    "客户端同步失败；旧配置已经恢复并重新载入。",
                    snapshot.SnapshotVersion);
            }
            return await RecoverPreparedChangeAsync(
                prepared,
                applyError.Code,
                string.IsNullOrWhiteSpace(applyError.Message)
                    ? "客户端同步失败，应用状态没有得到确认。"
                    : applyError.Message,
                snapshot.SnapshotVersion,
                cancellationToken);
        }
        if (!applied.Applied || !applied.Reloaded)
        {
            return await RecoverPreparedChangeAsync(
                prepared,
                "NP_ADAPTER_APPLY_UNCONFIRMED",
                "适配器后台没有确认候选已应用并重载。",
                snapshot.SnapshotVersion,
                cancellationToken);
        }

        VerifyChangeResponse verified;
        try
        {
            verified = await adapterClient.VerifyChangeAsync(
                prepared.ChangeId,
                cancellationToken);
        }
        catch (ControlServiceException exception)
        {
            return await RecoverPreparedChangeAsync(
                prepared,
                exception.Code,
                exception.UserMessage,
                snapshot.SnapshotVersion,
                cancellationToken);
        }
        var verificationError = verified.Error;
        var verificationContractError = VerificationContractError(verified);
        if (verificationError is not null
            || !verified.ConfigurationVerified
            || verificationContractError is not null)
        {
            return await RecoverPreparedChangeAsync(
                prepared,
                verificationError?.Code
                    ?? (verificationContractError is null
                        ? null
                        : "NP_ADAPTER_VERIFICATION_INVALID"),
                verificationError?.Message ?? verificationContractError,
                snapshot.SnapshotVersion,
                cancellationToken);
        }

        var pathMessage = verified.PathVerified
            ? "配置已载入，并取得本次规则的真实路径证据。"
            : "规则文件和客户端重载已经确认；尚未证明真实流量绕过第三方 VPN。";
        return new AdapterSyncResult(
            true,
            verified.PathVerified
                ? "NP_ADAPTER_PATH_VERIFIED"
                : "NP_ADAPTER_CONFIGURATION_VERIFIED",
            pathMessage,
            snapshot.SnapshotVersion,
            projection.RuleCount,
            prepared.ClientValidated,
            applied.Reloaded,
            verified.ConfigurationVerified,
            verified.PathVerified,
            verified.EvidenceLevel,
            Array.Empty<AdapterProjectionBlocker>());
    }

    private async Task<AdapterSyncResult> RecoverPreparedChangeAsync(
        PrepareChangeResponse prepared,
        string? failureCode,
        string? failureMessage,
        ulong snapshotVersion,
        CancellationToken cancellationToken)
    {
        RollbackChangeResponse rollback;
        try
        {
            rollback = await adapterClient.RollbackChangeAsync(
                prepared.ChangeId,
                prepared.BackupId,
                cancellationToken);
        }
        catch (ControlServiceException exception)
        {
            return AdapterSyncResult.Rejected(
                exception.Code,
                "候选状态无法确认，回滚请求也没有得到确认；请立即打开诊断并停止继续同步。",
                snapshotVersion);
        }
        var rollbackError = rollback.Error;
        if (rollbackError is not null
            || !rollback.Restored
            || !rollback.Reloaded)
        {
            return AdapterSyncResult.Rejected(
                rollbackError?.Code ?? "NP_ADAPTER_RECOVERY_INCOMPLETE",
                "候选状态无法确认，且无法确认旧配置已经完整恢复；请立即打开诊断并停止继续同步。",
                snapshotVersion);
        }
        return AdapterSyncResult.Rejected(
            string.IsNullOrWhiteSpace(failureCode)
                ? "NP_ADAPTER_CONFIGURATION_UNVERIFIED"
                : failureCode,
            string.IsNullOrWhiteSpace(failureMessage)
                ? "候选状态未通过确认，旧配置已经恢复并重新载入。"
                : $"{failureMessage} 旧配置已经恢复并重新载入。",
            snapshotVersion);
    }

    private static bool RequiresApplicationCatalog(
        GetActivePolicySnapshotResponse snapshot)
    {
        return snapshot.Policies.Any(policy =>
            policy.Enabled
            && policy.Decision?.Action == RouteAction.Direct
            && policy.Origin != NonProxy.Policy.V1.PolicyOrigin.System
            && policy.SourceKind != NonProxy.Policy.V1.PolicySourceKind.System
            && policy.Match?.App is not null);
    }

    private static bool SameSnapshot(
        GetActivePolicySnapshotResponse left,
        GetActivePolicySnapshotResponse right)
    {
        return left.SnapshotVersion == right.SnapshotVersion
            && left.ContentHash.Length == 32
            && right.ContentHash.Length == 32
            && CryptographicOperations.FixedTimeEquals(
                left.ContentHash.Span,
                right.ContentHash.Span);
    }

    private static void ValidateSnapshot(GetActivePolicySnapshotResponse response)
    {
        ThrowIfError(response.Error, "无法读取当前生效策略快照。");
        if (response.SnapshotVersion == 0)
        {
            throw new ControlServiceException(
                "NP_ADAPTER_ACTIVE_SNAPSHOT_REQUIRED",
                "还没有已经生效的策略快照，暂时不能同步第三方客户端。");
        }
        if (response.ContentHash.Length != 32)
        {
            throw InvalidContract("生效策略快照缺少 32 字节内容哈希。");
        }
    }

    private static void ValidatePrepared(
        PrepareChangeResponse response,
        int expectedRuleCount)
    {
        if (!response.ClientValidated
            || string.IsNullOrWhiteSpace(response.ChangeId)
            || string.IsNullOrWhiteSpace(response.BackupId)
            || response.CandidateHash.Length != 32
            || response.ConfigurationCandidateHash.Length != 32
            || response.RuleCount != checked((uint)expectedRuleCount)
            || response.ExpiresAt is null
            || string.IsNullOrWhiteSpace(response.ManagedRulesReference)
            || string.IsNullOrWhiteSpace(response.DirectTarget))
        {
            throw InvalidContract("适配器后台返回了不完整的候选校验结果。");
        }
    }

    private static string? VerificationContractError(
        VerifyChangeResponse response)
    {
        if (response.Verified != response.PathVerified)
        {
            return "适配器路径验证标志彼此矛盾。";
        }
        if (response.ConfigurationVerified
            && response.EvidenceLevel < EvidenceLevel.Configuration)
        {
            return "适配器配置验证缺少对应证据等级。";
        }
        if (response.PathVerified
            && response.EvidenceLevel < EvidenceLevel.Path)
        {
            return "适配器路径验证缺少路径证据等级。";
        }
        return null;
    }

    private static AdapterInstallationItem Map(AdapterInstallation installation)
    {
        if (string.IsNullOrWhiteSpace(installation.AdapterId)
            || installation.Client == AdapterClient.Unspecified
            || string.IsNullOrWhiteSpace(installation.ClientName)
            || string.IsNullOrWhiteSpace(installation.ExecutablePath)
            || string.IsNullOrWhiteSpace(installation.ManagedRulesPath)
            || string.IsNullOrWhiteSpace(installation.MainConfigurationPath))
        {
            throw InvalidContract("适配器目录包含不完整的安装项。");
        }
        return new AdapterInstallationItem(
            installation.AdapterId,
            installation.Client,
            installation.ClientName,
            installation.ClientVersion,
            installation.ExecutablePath,
            installation.ManagedRulesPath,
            installation.MainConfigurationPath,
            string.IsNullOrWhiteSpace(installation.DirectTarget)
                ? null
                : installation.DirectTarget,
            installation.State);
    }

    private static void ValidateRegistration(AdapterRegistrationDraft draft)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(draft.Id);
        if (draft.Client is not (AdapterClient.Surge
            or AdapterClient.Mihomo
            or AdapterClient.SingBox))
        {
            throw new ControlServiceException(
                "NP_ADAPTER_CLIENT_REQUIRED",
                "请选择受支持的第三方客户端类型。");
        }
        ValidatePath(draft.ExecutablePath, "客户端可执行文件");
        ValidatePath(draft.MainConfigurationPath, "客户端主配置");
        ValidatePath(draft.ManagedRulesPath, "NonProxy 托管规则文件");
    }

    private static void ValidatePath(string path, string fieldName)
    {
        if (string.IsNullOrWhiteSpace(path)
            || path.Length > 4_096
            || path.Any(char.IsControl))
        {
            throw new ControlServiceException(
                "NP_ADAPTER_PATH_INVALID",
                $"{fieldName}必须使用有效的绝对路径。");
        }
        try
        {
            if (Path.IsPathFullyQualified(path))
            {
                return;
            }
        }
        catch (Exception exception) when (
            exception is ArgumentException
                or NotSupportedException
                or PathTooLongException)
        {
        }
        throw new ControlServiceException(
            "NP_ADAPTER_PATH_INVALID",
            $"{fieldName}必须使用有效的绝对路径。");
    }

    private static AdapterMutationResult Rejected(
        ErrorDetail error,
        string fallback)
    {
        return new AdapterMutationResult(
            false,
            string.IsNullOrWhiteSpace(error.Code)
                ? "NP_ADAPTER_FAILED"
                : error.Code,
            string.IsNullOrWhiteSpace(error.Message) ? fallback : error.Message);
    }

    private static AdapterSyncResult RejectedSync(
        ErrorDetail error,
        string fallback,
        ulong snapshotVersion = 0)
    {
        return AdapterSyncResult.Rejected(
            string.IsNullOrWhiteSpace(error.Code)
                ? "NP_ADAPTER_FAILED"
                : error.Code,
            string.IsNullOrWhiteSpace(error.Message) ? fallback : error.Message,
            snapshotVersion);
    }

    private static void ThrowIfError(ErrorDetail? error, string fallback)
    {
        if (error is null)
        {
            return;
        }
        throw new ControlServiceException(
            string.IsNullOrWhiteSpace(error.Code)
                ? "NP_ADAPTER_FAILED"
                : error.Code,
            string.IsNullOrWhiteSpace(error.Message) ? fallback : error.Message);
    }

    private static ControlServiceException InvalidContract(string message)
    {
        return new ControlServiceException(
            "NP_ADAPTER_CONTRACT_INVALID",
            message);
    }
}
