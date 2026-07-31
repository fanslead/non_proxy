namespace NonProxy.Desktop.Core.Services.Control.Events;

public enum ControlEventKind
{
    StreamReady,
    SystemState,
    Snapshot,
    Decision,
    ComponentHealth,
    LearningCandidate,
    Unknown,
}

public sealed record ControlEventNotification(
    ulong Sequence,
    ControlEventKind Kind)
{
    public static ControlEventNotification Ready { get; } = new(
        0,
        ControlEventKind.StreamReady);
}
