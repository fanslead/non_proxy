namespace NonProxy.Desktop.Core.Platform;

internal sealed class UnavailableLocalProxyDiscovery : ILocalProxyDiscovery
{
    private const string UnavailableMessage =
        "当前平台尚未接入系统代理发现；仍可粘贴代理链接或手动填写。";

    public Task<LocalProxyDiscoverySnapshot> DiscoverAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(
            LocalProxyDiscoverySnapshot.Unavailable(UnavailableMessage));
    }
}
