namespace NonProxy.Desktop.Core.Services.Control;

public interface ISystemStatusService
{
    Task<SystemOverview> GetOverviewAsync(CancellationToken cancellationToken);
}

public interface IPolicyService
{
    Task<PolicyCatalog> GetCatalogAsync(CancellationToken cancellationToken);

    Task<ApplyResult> SaveAsync(
        PolicyDraft draft,
        CancellationToken cancellationToken);

    Task<ApplyResult> DeleteAsync(
        string policyId,
        CancellationToken cancellationToken);

    Task<ApplyResult> RollBackAsync(
        ulong snapshotVersion,
        CancellationToken cancellationToken);
}

public interface IOutboundService
{
    Task<OutboundCatalog> ListAsync(CancellationToken cancellationToken);

    Task<OutboundTestResult> TestAsync(
        string outboundId,
        CancellationToken cancellationToken);

    Task<ApplyResult> SetDefaultAsync(
        string outboundId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken);

    Task<ApplyResult> SetDirectAsync(
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken);

    Task<OutboundImportResult> ImportAsync(
        OutboundImportDraft draft,
        CancellationToken cancellationToken);
}

public interface ILearningService
{
    Task<LearningStatus> GetStatusAsync(CancellationToken cancellationToken);

    Task<LearningStatus> StartAsync(CancellationToken cancellationToken);

    Task<LearningStatus> StopAsync(CancellationToken cancellationToken);
}

public interface IActivityService
{
    Task<IReadOnlyList<ActivityItem>> GetRecentAsync(
        int limit,
        CancellationToken cancellationToken);
}

public interface IDiagnosticsService
{
    Task<IReadOnlyList<DiagnosticCheck>> RunChecksAsync(CancellationToken cancellationToken);
}

public interface IDesktopSettingsService
{
    Task<DesktopSettings> GetAsync(CancellationToken cancellationToken);

    Task SaveAsync(
        DesktopSettings settings,
        CancellationToken cancellationToken);
}
