namespace NonProxy.Desktop.Core.Platform;

public sealed record SystemComponentState
{
    public SystemComponentState(
        SystemComponentStatus status,
        string message,
        string? errorCode = null,
        IReadOnlyList<SystemComponentStep>? steps = null,
        bool canOpenSystemSettings = false)
    {
        Status = status;
        Message = message;
        ErrorCode = errorCode;
        Steps = steps?.ToArray() ?? Array.Empty<SystemComponentStep>();
        CanOpenSystemSettings = canOpenSystemSettings;
    }

    public SystemComponentStatus Status { get; }

    public string Message { get; }

    public string? ErrorCode { get; }

    public IReadOnlyList<SystemComponentStep> Steps { get; }

    public bool CanOpenSystemSettings { get; }
}
