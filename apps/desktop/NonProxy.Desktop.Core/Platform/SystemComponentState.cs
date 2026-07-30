namespace NonProxy.Desktop.Core.Platform;

public sealed record SystemComponentState(
    SystemComponentStatus Status,
    string Message,
    string? ErrorCode = null);
