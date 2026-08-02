using System.Globalization;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed record OutboundGroupMemberItem(
    int Position,
    string OutboundId,
    string Name,
    string Kind,
    string Endpoint,
    string Health)
{
    public string PositionLabel => Position.ToString(
        "00",
        CultureInfo.InvariantCulture);

    public static OutboundGroupMemberItem Create(
        int position,
        string outboundId,
        OutboundListItem? outbound)
    {
        return new OutboundGroupMemberItem(
            position,
            outboundId,
            outbound?.Name ?? outboundId,
            outbound?.Kind ?? "成员不可用",
            outbound?.Endpoint ?? "请刷新出口列表",
            outbound?.Health ?? "不可用");
    }
}

public sealed record OutboundGroupDefaultRouteChange(
    string GroupId,
    string GroupName,
    ulong RoutingRevision);
