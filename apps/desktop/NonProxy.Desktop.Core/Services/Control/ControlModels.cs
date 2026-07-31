using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Services.Control;

public enum ConnectionState
{
    Disconnected,
    Connecting,
    Connected,
    Interrupted,
}

public sealed record SystemOverview(
    ConnectionState Connection,
    SystemComponentState ComponentState,
    string Headline,
    string Detail,
    ulong? ActiveSnapshotVersion,
    int DirectApplicationCount,
    int DirectWebsiteCount,
    int RecentDecisionCount,
    DateTimeOffset CapturedAt,
    ulong? PendingSnapshotVersion = null)
{
    public SystemComponentStatus Component => ComponentState.Status;

    public static SystemOverview Unavailable(SystemComponentState component)
    {
        return new SystemOverview(
            ConnectionState.Disconnected,
            component,
            "等待控制服务",
            component.Message,
            null,
            0,
            0,
            0,
            DateTimeOffset.UtcNow);
    }
}

public enum PolicyScope
{
    Application,
    Website,
    ApplicationAndDestination,
}

public enum PolicyAction
{
    Direct,
    Proxy,
    Block,
}

public enum PolicyApplyState
{
    Draft,
    Pending,
    Active,
    PendingRemoval,
    Rejected,
}

public sealed record PolicyListItem(
    string Id,
    string Name,
    PolicyScope Scope,
    string MatchValue,
    PolicyAction Action,
    PolicyApplyState State,
    ulong? SnapshotVersion,
    DateTimeOffset? UpdatedAt,
    ulong Revision = 1,
    ulong? EffectiveRevision = null,
    ulong? PendingRevision = null)
{
    public string ScopeLabel => Scope switch
    {
        PolicyScope.Application => "应用",
        PolicyScope.Website => "网站",
        PolicyScope.ApplicationAndDestination => "应用 + 目标",
        _ => "未知",
    };

    public string ActionLabel => Action switch
    {
        PolicyAction.Direct => "直连",
        PolicyAction.Proxy => "代理",
        PolicyAction.Block => "阻止",
        _ => "未知",
    };

    public string StateLabel => State switch
    {
        PolicyApplyState.Active => "已应用",
        PolicyApplyState.Pending => "等待系统组件确认",
        PolicyApplyState.PendingRemoval => "等待从数据面移除",
        PolicyApplyState.Rejected => "应用失败",
        _ => "草稿",
    };
}

public sealed record PolicyCatalog(
    IReadOnlyList<PolicyListItem> Items,
    ulong? ActiveSnapshotVersion,
    DateTimeOffset CapturedAt,
    ulong? PendingSnapshotVersion = null)
{
    public static PolicyCatalog Empty { get; } = new(
        Array.Empty<PolicyListItem>(),
        null,
        DateTimeOffset.MinValue);
}

public sealed record PolicyDraft(
    string? ExistingId,
    string Name,
    PolicyScope Scope,
    string MatchValue,
    PolicyAction Action,
    ulong? ExistingRevision = null,
    string? Destination = null,
    string? OutboundId = null,
    string? ApplicationSignerId = null,
    bool IncludeApplicationHelpers = false);

public sealed record ApplyResult(
    bool Accepted,
    bool Applied,
    string Code,
    string Message,
    ulong? SnapshotVersion)
{
    public static ApplyResult Unavailable { get; } = new(
        false,
        false,
        "NP_CONTROL_UNAVAILABLE",
        "控制服务尚未连接，规则没有保存，也没有应用到网络。",
        null);
}

public sealed record OutboundListItem(
    string Id,
    string Name,
    string Kind,
    string Endpoint,
    string Health,
    DateTimeOffset? LastCheckedAt);

public enum OutboundProxyKind
{
    Socks5,
    HttpConnect,
}

public sealed record OutboundImportDraft(
    string Id,
    OutboundProxyKind Kind,
    string Host,
    uint Port,
    string? Username,
    string? Password);

public sealed record OutboundImportResult(
    string ImportId,
    IReadOnlyList<OutboundListItem> Outbounds,
    IReadOnlyList<string> Warnings);

public sealed record LearningStatus(
    bool IsRunning,
    int CandidateCount,
    DateTimeOffset? StartedAt,
    string Detail);

public sealed record ActivityItem(
    long Sequence,
    DateTimeOffset OccurredAt,
    string Application,
    string Destination,
    string Action,
    string Reason);

public sealed record DiagnosticCheck(
    string Id,
    string Name,
    string Status,
    string Detail);

public sealed record DesktopSettings(
    string Theme,
    bool StartAtLogin,
    bool KeepDataPlaneRunning,
    bool CollectDecisionHistory);
