namespace NonProxy.Desktop.Core.Platform;

public sealed record InstallResult(
    bool Success,
    string Message,
    string? ErrorCode = null)
{
    public static InstallResult Unavailable(string message, string errorCode)
    {
        return new InstallResult(false, message, errorCode);
    }
}
