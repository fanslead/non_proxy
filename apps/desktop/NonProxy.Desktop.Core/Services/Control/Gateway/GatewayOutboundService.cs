using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Events.V1;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayOutboundService : IOutboundService
{
    private const int MaximumPages = 100;

    private readonly IControlRpcClient _client;

    public GatewayOutboundService(IControlRpcClient client)
    {
        _client = client;
    }

    public async Task<IReadOnlyList<OutboundListItem>> ListAsync(
        CancellationToken cancellationToken)
    {
        var items = new List<OutboundListItem>();
        var tokens = new HashSet<string>(StringComparer.Ordinal);
        var pageToken = string.Empty;
        for (var page = 0; page < MaximumPages; page++)
        {
            if (!tokens.Add(pageToken))
            {
                throw InvalidPaging();
            }

            var response = await _client.ListOutboundsAsync(
                pageToken,
                cancellationToken);
            items.AddRange(response.Outbounds.Select(ToItem));
            pageToken = response.Page?.NextPageToken ?? string.Empty;
            if (string.IsNullOrEmpty(pageToken))
            {
                return items;
            }
        }

        throw InvalidPaging();
    }

    private static OutboundListItem ToItem(OutboundSummary outbound)
    {
        return new OutboundListItem(
            outbound.Id,
            outbound.DisplayName,
            KindLabel(outbound.Kind),
            "端点与凭据由本地安全存储管理",
            HealthLabel(outbound.Health),
            null);
    }

    private static string KindLabel(OutboundKind kind)
    {
        return kind switch
        {
            OutboundKind.Direct => "直连",
            OutboundKind.HttpConnect => "HTTP CONNECT",
            OutboundKind.Socks5 => "SOCKS5",
            OutboundKind.Wireguard => "WireGuard",
            OutboundKind.Openvpn => "OpenVPN",
            OutboundKind.ExternalAdapter => "外部适配器",
            _ => "未知",
        };
    }

    private static string HealthLabel(RuntimeState state)
    {
        return state switch
        {
            RuntimeState.Ready => "可用",
            RuntimeState.Degraded => "降级",
            RuntimeState.Starting => "启动中",
            RuntimeState.Failed => "异常",
            RuntimeState.Stopped => "未启动",
            _ => "未验证",
        };
    }

    private static ControlServiceException InvalidPaging()
    {
        return new ControlServiceException(
            "NP_CONTROL_PAGING_INVALID",
            "控制服务返回了无效出口分页游标。");
    }
}
