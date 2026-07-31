using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Mac;

internal sealed class MacLocalProxyDiscovery(
    MacNativeBridgeClient nativeBridge) : ILocalProxyDiscovery
{
    public async Task<LocalProxyDiscoverySnapshot> DiscoverAsync(
        CancellationToken cancellationToken)
    {
        try
        {
            var payload = await nativeBridge.DiscoverSystemProxiesAsync(
                cancellationToken);
            if (!payload.Success)
            {
                return LocalProxyDiscoverySnapshot.Unavailable(payload.Message);
            }

            var candidates = payload.Proxies
                .Select(Map)
                .Where(candidate => candidate is not null)
                .Cast<LocalProxyCandidate>()
                .ToArray();
            if (payload.Proxies.Count > 0 && candidates.Length == 0)
            {
                return LocalProxyDiscoverySnapshot.Unavailable(
                    "系统代理设置没有通过本地安全校验，未生成任何候选。请改用代理链接或手动填写。");
            }
            var message = candidates.Length == payload.Proxies.Count
                ? payload.Message
                : $"{payload.Message} 已忽略 {payload.Proxies.Count - candidates.Length} 个无效端点。";
            return new LocalProxyDiscoverySnapshot(
                candidates,
                true,
                message);
        }
        catch (MacNativeBridgeException exception)
        {
            return LocalProxyDiscoverySnapshot.Unavailable(exception.Message);
        }
    }

    private static LocalProxyCandidate? Map(MacSystemProxyDescriptor value)
    {
        var protocol = value.Kind switch
        {
            "socks5" => LocalProxyProtocol.Socks5,
            "http_connect" => LocalProxyProtocol.HttpConnect,
            _ => (LocalProxyProtocol?)null,
        };
        if (protocol is null
            || string.IsNullOrWhiteSpace(value.SuggestedId)
            || string.IsNullOrWhiteSpace(value.DisplayName)
            || string.IsNullOrWhiteSpace(value.Host)
            || value.Port == 0
            || value.DisplayName.Length > 128
            || value.Host.Length > 253
            || value.DisplayName.Any(char.IsControl)
            || value.Host.Any(char.IsControl)
            || Uri.CheckHostName(value.Host.Trim()) == UriHostNameType.Unknown
            || value.SuggestedId.Length > 128
            || value.SuggestedId.Any(character =>
                !char.IsAsciiLetterOrDigit(character)
                && character is not '-' and not '_' and not '.'))
        {
            return null;
        }

        return new LocalProxyCandidate(
            value.SuggestedId,
            value.DisplayName.Trim(),
            protocol.Value,
            value.Host.Trim(),
            value.Port);
    }
}
