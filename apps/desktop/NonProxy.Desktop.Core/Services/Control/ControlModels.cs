using System.Globalization;
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
    int DirectNetworkCount,
    int RecentDecisionCount,
    DateTimeOffset CapturedAt,
    ulong? PendingSnapshotVersion = null,
    bool DataPlaneEnabled = false,
    int ActiveDirectRuleCount = 0)
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
            0,
            DateTimeOffset.UtcNow);
    }
}

public enum PolicyScope
{
    Application,
    Website,
    ApplicationAndDestination,
    Network,
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
        PolicyScope.Network => "网络环境",
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
    ulong? PendingSnapshotVersion = null,
    ulong? PreviousEffectiveSnapshotVersion = null)
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
    TimeSpan? Latency,
    DateTimeOffset? LastCheckedAt,
    bool IsDefault = false,
    bool SupportsDefaultRoute = false,
    bool IsHandshakeVerified = false,
    bool CanVerifyExit = false,
    ExitVerificationReceipt? ExitReceipt = null)
{
    public bool CanSetAsDefault =>
        !IsDefault && SupportsDefaultRoute && IsHandshakeVerified;

    public string DefaultLabel => IsDefault ? "默认代理配置" : string.Empty;

    public string DefaultEligibilityLabel => (IsDefault, SupportsDefaultRoute, IsHandshakeVerified)
        switch
    {
        (false, false, _) => "不支持全局默认",
        (false, true, false) => "需先通过握手测试",
        _ => string.Empty,
    };

    public string LatencyLabel => Latency is { } value
        ? $"{Math.Ceiling(value.TotalMilliseconds):0} ms"
        : "—";

    public string LastCheckedLabel => LastCheckedAt is { } value
        ? value.ToLocalTime().ToString("MM-dd HH:mm:ss", CultureInfo.CurrentCulture)
        : "尚未检查";

    public string ExitStatusLabel => ExitReceipt is null
        ? "尚未验证"
        : $"最近签名回执 · {ExitReceipt.ObservedIp}";

    public string ExitCheckedLabel => ExitReceipt is { } value
        ? value.VerifiedAt.ToLocalTime().ToString(
            "MM-dd HH:mm:ss",
            CultureInfo.CurrentCulture)
        : "—";
}

public sealed record OutboundCatalog(
    IReadOnlyList<OutboundListItem> Items,
    ulong RoutingRevision,
    string? DefaultOutboundId = null,
    bool ExitVerificationAvailable = false,
    ExitVerificationReceipt? DirectExitReceipt = null)
{
    public bool UsesDirectByDefault => DefaultOutboundId is null;
}

public sealed record OutboundTestResult(
    string OutboundId,
    bool Healthy,
    string Health,
    TimeSpan? Latency,
    DateTimeOffset CheckedAt,
    string Message);

public sealed record ExitVerificationReceipt(
    long Sequence,
    string ProbeId,
    string ObservedIp,
    DateTimeOffset ObservedAt,
    DateTimeOffset VerifiedAt,
    string? OutboundId);

public sealed record ExitVerificationResult(
    bool Verified,
    string Code,
    string Message);

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

public sealed record ActivityItem(
    long Sequence,
    DateTimeOffset OccurredAt,
    string Application,
    string Destination,
    string Action,
    string Reason,
    string Evidence,
    string Path,
    string Error,
    ulong SnapshotVersion)
{
    public string ResultLabel => $"{Action} · {Evidence}";

    public string OccurredAtLabel => OccurredAt
        .ToLocalTime()
        .ToString("MM-dd HH:mm:ss", CultureInfo.CurrentCulture);

    public string SnapshotLabel => SnapshotVersion == 0
        ? string.Empty
        : $"快照 v{SnapshotVersion}";

    public bool HasError => !string.IsNullOrEmpty(Error);
}

public sealed record DiagnosticCheck(
    string Id,
    string Name,
    string Status,
    string Detail);

public sealed record DiagnosticExport(
    string DiagnosticId,
    string LocalPath,
    long SizeBytes,
    string Sha256,
    string Redaction,
    DateTimeOffset RangeStart,
    DateTimeOffset RangeEnd,
    IReadOnlyList<string> IncludedSections,
    int ConnectionSampleCount,
    int ErrorCount)
{
    public string SizeLabel => SizeBytes < 1024
        ? $"{SizeBytes} B"
        : $"{SizeBytes / 1024d:0.0} KiB";

    public string RangeLabel =>
        $"{RangeStart.ToLocalTime():MM-dd HH:mm} — {RangeEnd.ToLocalTime():MM-dd HH:mm}";

    public string Summary =>
        $"{Redaction}；连接样本 {ConnectionSampleCount} 条，错误记录 {ErrorCount} 条。";
}
