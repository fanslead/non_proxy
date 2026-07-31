namespace NonProxy.Desktop.Core.Platform;

public enum NetworkFingerprintKind
{
    WiFiSsidSha256,
    DefaultGatewaySha256,
    InterfaceClass,
}

public sealed record CurrentNetworkEnvironment(
    bool IsAvailable,
    string SuggestedName,
    NetworkFingerprintKind? FingerprintKind,
    string? FingerprintValue,
    string PermissionState,
    string Message)
{
    public static CurrentNetworkEnvironment Unavailable(string message)
    {
        return new CurrentNetworkEnvironment(
            false,
            string.Empty,
            null,
            null,
            "unavailable",
            message);
    }
}

public interface ICurrentNetworkEnvironment
{
    Task<CurrentNetworkEnvironment> CaptureAsync(
        CancellationToken cancellationToken);
}
