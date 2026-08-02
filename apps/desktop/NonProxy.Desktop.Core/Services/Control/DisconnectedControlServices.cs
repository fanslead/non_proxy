using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Services.Control;

public sealed class DisconnectedSystemStatusService : ISystemStatusService
{
    private readonly ISystemComponentInstaller _installer;

    public DisconnectedSystemStatusService(ISystemComponentInstaller installer)
    {
        _installer = installer;
    }

    public async Task<SystemOverview> GetOverviewAsync(CancellationToken cancellationToken)
    {
        var component = await _installer.GetStateAsync(cancellationToken);
        return SystemOverview.Unavailable(component);
    }
}

public sealed class DisconnectedPolicyService : IPolicyService
{
    public Task<PolicyCatalog> GetCatalogAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(PolicyCatalog.Empty with
        {
            CapturedAt = DateTimeOffset.UtcNow,
        });
    }

    public Task<ApplyResult> SaveAsync(
        PolicyDraft draft,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(draft);
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ApplyResult.Unavailable);
    }

    public Task<ApplyResult> DeleteAsync(
        string policyId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(policyId);
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ApplyResult.Unavailable);
    }

    public Task<ApplyResult> RollBackAsync(
        ulong snapshotVersion,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ApplyResult.Unavailable);
    }
}

public sealed class DisconnectedOutboundService : IOutboundService
{
    public Task<OutboundCatalog> ListAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(new OutboundCatalog(
            Array.Empty<OutboundListItem>(),
            0));
    }

    public Task<OutboundImportResult> ImportAsync(
        OutboundImportDraft draft,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(draft);
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "控制服务尚未连接，代理配置没有保存。");
    }

    public Task<OutboundImportResult> PreviewUriListAsync(
        string uriList,
        CancellationToken cancellationToken)
    {
        return ImportUriListAsync(uriList, cancellationToken);
    }

    public Task<OutboundImportResult> ImportUriListAsync(
        string uriList,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "控制服务尚未连接，代理链接没有检查或保存。");
    }

    public Task<OutboundTestResult> TestAsync(
        string outboundId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "控制服务尚未连接，无法测试代理握手。");
    }

    public Task<ExitVerificationResult> VerifyExitAsync(
        string? outboundId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "控制服务尚未连接，无法验证公网出口。");
    }

    public Task<ApplyResult> SetDefaultAsync(
        string outboundId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ApplyResult.Unavailable);
    }

    public Task<ApplyResult> SetDirectAsync(
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(ApplyResult.Unavailable);
    }
}

public sealed class DisconnectedOutboundGroupService : IOutboundGroupService
{
    public Task<OutboundGroupCatalog> ListAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(new OutboundGroupCatalog([], 0));
    }

    public Task<OutboundGroupMutation> SaveAsync(
        OutboundGroupDraft draft,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(draft);
        return Unavailable<OutboundGroupMutation>(cancellationToken);
    }

    public Task<OutboundGroupDeletion> DeleteAsync(
        string groupId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(groupId);
        return Unavailable<OutboundGroupDeletion>(cancellationToken);
    }

    public Task<ApplyResult> SetDefaultAsync(
        string groupId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(groupId);
        return Unavailable<ApplyResult>(cancellationToken);
    }

    private static Task<TResult> Unavailable<TResult>(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "控制服务尚未连接，自动切换线路组没有发生变化。");
    }
}

public sealed class DisconnectedSubscriptionService : ISubscriptionService
{
    public Task<SubscriptionCatalog> ListAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(SubscriptionCatalog.Empty with
        {
            CapturedAt = DateTimeOffset.UtcNow,
        });
    }

    public Task<SubscriptionMutation> SaveAsync(
        SubscriptionDraft draft,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(draft);
        return Unavailable<SubscriptionMutation>(cancellationToken);
    }

    public Task<SubscriptionMutation> RefreshAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(sourceId);
        return Unavailable<SubscriptionMutation>(cancellationToken);
    }

    public Task<SubscriptionDeletion> DeleteAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(sourceId);
        return Unavailable<SubscriptionDeletion>(cancellationToken);
    }

    private static Task<TResult> Unavailable<TResult>(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "控制服务尚未连接，订阅没有发生变化。");
    }
}

public sealed class DisconnectedActivityService : IActivityService
{
    public Task<IReadOnlyList<ActivityItem>> GetRecentAsync(
        int limit,
        CancellationToken cancellationToken)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(limit);
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult<IReadOnlyList<ActivityItem>>(Array.Empty<ActivityItem>());
    }
}

public sealed class DisconnectedDiagnosticsService : IDiagnosticsService
{
    private readonly ISystemComponentInstaller _installer;

    public DisconnectedDiagnosticsService(ISystemComponentInstaller installer)
    {
        _installer = installer;
    }

    public async Task<IReadOnlyList<DiagnosticCheck>> RunChecksAsync(
        CancellationToken cancellationToken)
    {
        var component = await _installer.GetStateAsync(cancellationToken);
        var checks = new List<DiagnosticCheck>
        {
            new(
                "control-service",
                "控制服务",
                "未连接",
                "本地控制服务尚未打包或启动。"),
        };
        SystemComponentDiagnostics.AddTo(checks, component);
        return checks;
    }

    public Task<DiagnosticExport> ExportAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "控制服务尚未连接，诊断包没有生成。");
    }
}
