namespace NonProxy.Desktop.Core.Services.Control;

public interface ISystemStatusService
{
    Task<SystemOverview> GetOverviewAsync(CancellationToken cancellationToken);
}

public interface IRuntimeOverrideService
{
    Task<RuntimeOverrideStatus> GetStatusAsync(
        CancellationToken cancellationToken);

    Task<ApplyResult> SetAsync(
        RuntimeOverrideKind kind,
        string? outboundId,
        TimeSpan duration,
        CancellationToken cancellationToken);

    Task<ApplyResult> ClearAsync(CancellationToken cancellationToken);
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

public interface INetworkProfileService
{
    Task<NetworkProfileCatalog> GetCatalogAsync(
        CancellationToken cancellationToken);

    Task<NetworkProfileMutation> SaveAsync(
        NetworkProfileDraft draft,
        CancellationToken cancellationToken);

    Task<NetworkProfileMutation> DeleteAsync(
        string profileId,
        ulong expectedRevision,
        CancellationToken cancellationToken);
}

public interface IOutboundService
{
    Task<OutboundCatalog> ListAsync(CancellationToken cancellationToken);

    Task<OutboundTestResult> TestAsync(
        string outboundId,
        CancellationToken cancellationToken);

    Task<ExitVerificationResult> VerifyExitAsync(
        string? outboundId,
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

    Task<OutboundImportResult> PreviewUriListAsync(
        string uriList,
        CancellationToken cancellationToken);

    Task<OutboundImportResult> ImportUriListAsync(
        string uriList,
        CancellationToken cancellationToken);
}

public interface IOutboundGroupService
{
    Task<OutboundGroupCatalog> ListAsync(CancellationToken cancellationToken);

    Task<OutboundGroupMutation> SaveAsync(
        OutboundGroupDraft draft,
        CancellationToken cancellationToken);

    Task<OutboundGroupDeletion> DeleteAsync(
        string groupId,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<ApplyResult> SetDefaultAsync(
        string groupId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken);
}

public interface ISubscriptionService
{
    Task<SubscriptionCatalog> ListAsync(CancellationToken cancellationToken);

    Task<SubscriptionMutation> SaveAsync(
        SubscriptionDraft draft,
        CancellationToken cancellationToken);

    Task<SubscriptionMutation> RefreshAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken);

    Task<SubscriptionDeletion> DeleteAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken);
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

    Task<DiagnosticExport> ExportAsync(CancellationToken cancellationToken);
}
