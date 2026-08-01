using System.Globalization;

namespace NonProxy.Desktop.Core.Services.Control;

public enum RuntimeOverrideKind
{
    Paused,
    Direct,
    Proxy,
}

public sealed record RuntimeOverrideInfo(
    RuntimeOverrideKind Kind,
    string? OutboundId,
    DateTimeOffset ExpiresAt)
{
    public string ModeLabel => Kind switch
    {
        RuntimeOverrideKind.Paused => "暂停 NonProxy",
        RuntimeOverrideKind.Direct => "全部直连",
        RuntimeOverrideKind.Proxy => "全部代理",
        _ => "未知覆盖",
    };

    public string ExpiryLabel => ExpiresAt.ToLocalTime().ToString(
        "HH:mm:ss",
        CultureInfo.CurrentCulture);
}

public sealed record RuntimeOverrideStatus(
    bool IsAvailable,
    RuntimeOverrideInfo? Active,
    RuntimeOverrideInfo? Pending,
    ulong? ActiveSnapshotVersion,
    ulong? PendingSnapshotVersion,
    bool PendingClearsOverride)
{
    public bool HasPendingMutation => PendingSnapshotVersion is not null;

    public bool CanRequest => IsAvailable
        && ActiveSnapshotVersion is not null
        && !HasPendingMutation;

    public bool CanClear => CanRequest && Active is not null;

    public static RuntimeOverrideStatus Unavailable { get; } = new(
        false,
        null,
        null,
        null,
        null,
        false);
}
