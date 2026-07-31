using Google.Protobuf.WellKnownTypes;
using NonProxy.Control.V1;
using NonProxy.Events.V1;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

internal static class OutboundContractMapper
{
    private static readonly TimeSpan MaximumProbeLatency = TimeSpan.FromSeconds(30);

    public static OutboundListItem ToItem(OutboundSummary outbound)
    {
        return new OutboundListItem(
            outbound.Id,
            string.IsNullOrWhiteSpace(outbound.DisplayName)
                ? outbound.Id
                : outbound.DisplayName,
            KindLabel(outbound.Kind),
            EndpointLabel(outbound.EndpointHost, outbound.EndpointPort),
            HealthLabel(outbound.Health),
            outbound.Latency is null ? null : ToTimeSpan(outbound.Latency),
            outbound.LastCheckedAt is null
                ? null
                : ToDateTimeOffset(outbound.LastCheckedAt),
            outbound.IsDefault,
            SupportsDefaultRoute(outbound));
    }

    public static TimeSpan ToTimeSpan(Duration value)
    {
        try
        {
            var result = value.ToTimeSpan();
            if (result < TimeSpan.Zero || result > MaximumProbeLatency)
            {
                throw InvalidProbeContract();
            }

            return result;
        }
        catch (InvalidOperationException exception)
        {
            throw InvalidProbeContract(exception);
        }
    }

    public static ControlServiceException InvalidProbeContract(
        Exception? innerException = null)
    {
        return new ControlServiceException(
            "NP_CONTROL_CONTRACT_INVALID",
            "控制服务返回了无效的代理测试结果。",
            innerException);
    }

    private static bool SupportsDefaultRoute(OutboundSummary outbound)
    {
        return outbound.Enabled
            && outbound.Capabilities.Contains(CapabilityName.Tcp)
            && outbound.Capabilities.Contains(CapabilityName.Udp)
            && outbound.Capabilities.Contains(CapabilityName.Ipv4)
            && outbound.Capabilities.Contains(CapabilityName.Ipv6);
    }

    private static string EndpointLabel(string host, uint port)
    {
        return string.IsNullOrWhiteSpace(host) || port == 0
            ? "由本地适配器管理"
            : $"{host}:{port}";
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
            RuntimeState.Ready => "代理握手可用",
            RuntimeState.Degraded => "握手降级",
            RuntimeState.Starting => "检测中",
            RuntimeState.Failed => "握手异常",
            RuntimeState.Stopped => "未验证",
            _ => "未验证",
        };
    }

    private static DateTimeOffset ToDateTimeOffset(Timestamp value)
    {
        try
        {
            return value.ToDateTimeOffset();
        }
        catch (InvalidOperationException exception)
        {
            throw InvalidProbeContract(exception);
        }
    }
}
