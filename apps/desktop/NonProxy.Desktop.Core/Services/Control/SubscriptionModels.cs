namespace NonProxy.Desktop.Core.Services.Control;

public sealed record SubscriptionListItem(
    string Id,
    string DisplayName,
    bool Enabled,
    TimeSpan RefreshInterval,
    ulong Revision,
    ulong ContentGeneration,
    uint ConsecutiveFailures,
    DateTimeOffset NextRefreshAt,
    DateTimeOffset? LastAttemptedAt,
    DateTimeOffset? LastSucceededAt,
    string? LastErrorCode,
    uint NodeCount);

public sealed record SubscriptionCatalog(
    IReadOnlyList<SubscriptionListItem> Items,
    DateTimeOffset CapturedAt)
{
    public static SubscriptionCatalog Empty { get; } = new(
        Array.Empty<SubscriptionListItem>(),
        DateTimeOffset.MinValue);
}

public sealed record SubscriptionDraft(
    string Id,
    string DisplayName,
    string? EndpointUrl,
    bool Enabled,
    TimeSpan RefreshInterval,
    ulong? ExpectedRevision);

public sealed record SubscriptionMutation(
    bool Accepted,
    string Code,
    string Message,
    SubscriptionListItem? Item,
    bool ContentUnchanged,
    IReadOnlyList<string> Warnings);

public sealed record SubscriptionDeletion(
    bool Accepted,
    string Code,
    string Message,
    string? SourceId,
    uint RemovedOutboundCount,
    IReadOnlyList<string> Warnings);
