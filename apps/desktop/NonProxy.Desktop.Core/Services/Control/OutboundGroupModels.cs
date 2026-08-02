namespace NonProxy.Desktop.Core.Services.Control;

public sealed record OutboundGroupListItem(
    string Id,
    string Name,
    IReadOnlyList<string> OutboundIds,
    ulong Revision,
    bool IsDefault = false,
    IReadOnlyList<string>? MemberNames = null)
{
    public int MemberCount => OutboundIds.Count;

    public string MemberCountLabel => $"{MemberCount} 条线路";

    public string PrioritySummary => string.Join(
        "  →  ",
        MemberNames ?? OutboundIds);

    public string DefaultLabel => IsDefault ? "当前默认" : string.Empty;
}

public sealed record OutboundGroupCatalog(
    IReadOnlyList<OutboundGroupListItem> Groups,
    ulong RoutingRevision,
    string? DefaultGroupId = null);

public sealed record OutboundGroupDraft(
    string Id,
    string Name,
    IReadOnlyList<string> OutboundIds,
    ulong? ExpectedRevision = null);

public sealed record OutboundGroupMutation(
    bool Accepted,
    string Code,
    string Message,
    OutboundGroupListItem? Group,
    ulong RoutingRevision,
    ulong? PendingSnapshotVersion = null);

public sealed record OutboundGroupDeletion(
    bool Accepted,
    string Code,
    string Message);
