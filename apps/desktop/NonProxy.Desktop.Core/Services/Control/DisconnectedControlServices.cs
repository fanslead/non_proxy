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
    public Task<IReadOnlyList<OutboundListItem>> ListAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult<IReadOnlyList<OutboundListItem>>(
            Array.Empty<OutboundListItem>());
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
}

public sealed class DisconnectedLearningService : ILearningService
{
    private static LearningStatus Unavailable { get; } = new(
        false,
        0,
        null,
        "控制服务尚未连接，学习模式没有启动。");

    public Task<LearningStatus> GetStatusAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(Unavailable);
    }

    public Task<LearningStatus> StartAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(Unavailable);
    }

    public Task<LearningStatus> StopAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(Unavailable);
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
}

public sealed class DisconnectedDesktopSettingsService : IDesktopSettingsService
{
    private static DesktopSettings Defaults { get; } = new(
        "System",
        false,
        true,
        true);

    public Task<DesktopSettings> GetAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(Defaults);
    }

    public Task SaveAsync(
        DesktopSettings settings,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(settings);
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "控制服务尚未连接，设置没有保存。");
    }
}
