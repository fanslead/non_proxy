using System.Net;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed partial class OutboundsViewModel
{
    [ObservableProperty]
    private string? _localProxyDiscoveryMessage;

    public IAsyncRelayCommand DiscoverLocalProxiesCommand { get; private set; } = null!;

    private void InitializeLocalProxyDiscoveryCommand()
    {
        DiscoverLocalProxiesCommand = new AsyncRelayCommand(
            DiscoverLocalProxiesAsync,
            () => !IsBusy);
    }

    private async Task DiscoverLocalProxiesAsync(
        CancellationToken cancellationToken)
    {
        if (IsBusy)
        {
            return;
        }

        var sourceAtStart = UriImportText;
        string? discoveredSource = null;
        await RunOperationAsync(
            async token =>
            {
                var snapshot = await _localProxyDiscovery.DiscoverAsync(token);
                if (!snapshot.IsAvailable || snapshot.Candidates.Count == 0)
                {
                    LocalProxyDiscoveryMessage = snapshot.Message;
                    return;
                }
                if (!string.Equals(
                        UriImportText,
                        sourceAtStart,
                        StringComparison.Ordinal))
                {
                    LocalProxyDiscoveryMessage =
                        $"{snapshot.Message} 当前输入已变化，因此没有自动替换。";
                    return;
                }

                discoveredSource = string.Join(
                    Environment.NewLine,
                    snapshot.Candidates.Select(ToProxyUri));
                UriImportText = discoveredSource;
                LocalProxyDiscoveryMessage = snapshot.Message;
            },
            cancellationToken);

        if (discoveredSource is not null
            && string.Equals(
                UriImportText,
                discoveredSource,
                StringComparison.Ordinal)
            && !cancellationToken.IsCancellationRequested)
        {
            await PreviewUriImportAsync(cancellationToken);
        }
    }

    private static string ToProxyUri(LocalProxyCandidate candidate)
    {
        if (Uri.CheckHostName(candidate.Host) == UriHostNameType.Unknown
            || candidate.Port == 0
            || string.IsNullOrWhiteSpace(candidate.SuggestedId))
        {
            throw new InvalidOperationException("平台返回了无效的系统代理端点。");
        }
        var scheme = candidate.Protocol switch
        {
            LocalProxyProtocol.Socks5 => "socks5",
            LocalProxyProtocol.HttpConnect => "http",
            _ => throw new ArgumentOutOfRangeException(nameof(candidate)),
        };
        var host = IPAddress.TryParse(candidate.Host, out var address)
            && address.AddressFamily
                == System.Net.Sockets.AddressFamily.InterNetworkV6
                ? $"[{candidate.Host}]"
                : candidate.Host;
        var label = Uri.EscapeDataString(candidate.SuggestedId);
        return $"{scheme}://{host}:{candidate.Port}#{label}";
    }
}
