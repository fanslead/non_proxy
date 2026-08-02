namespace NonProxy.Desktop.Core.Features.Subscriptions;

public sealed record SubscriptionIntervalOption(
    string Label,
    TimeSpan Value,
    string Hint);
