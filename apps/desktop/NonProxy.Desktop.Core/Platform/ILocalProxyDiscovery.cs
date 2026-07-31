namespace NonProxy.Desktop.Core.Platform;

public enum LocalProxyProtocol
{
    Socks5,
    HttpConnect,
}

public sealed record LocalProxyCandidate(
    string SuggestedId,
    string DisplayName,
    LocalProxyProtocol Protocol,
    string Host,
    ushort Port);

public sealed record LocalProxyDiscoverySnapshot(
    IReadOnlyList<LocalProxyCandidate> Candidates,
    bool IsAvailable,
    string Message)
{
    public static LocalProxyDiscoverySnapshot Unavailable(string message)
    {
        return new(
            Array.Empty<LocalProxyCandidate>(),
            false,
            message);
    }
}

public interface ILocalProxyDiscovery
{
    Task<LocalProxyDiscoverySnapshot> DiscoverAsync(
        CancellationToken cancellationToken);
}
