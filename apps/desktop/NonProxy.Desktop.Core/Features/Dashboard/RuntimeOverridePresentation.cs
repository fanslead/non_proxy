using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Dashboard;

public sealed record RuntimeOverridePanelState(
    string Headline,
    string Detail,
    bool IsAvailable,
    bool HasActiveOverride,
    bool HasPendingMutation,
    bool CanRequest,
    bool CanProxy,
    bool CanClear)
{
    public static RuntimeOverridePanelState Loading { get; } = new(
        "正在读取限时运行模式",
        "状态确认前不会声称操作已经生效。",
        false,
        false,
        false,
        false,
        false,
        false);

    internal static RuntimeOverridePanelState Build(
        OptionalRead<RuntimeOverrideStatus> status,
        OptionalRead<OutboundCatalog> outbounds)
    {
        if (!status.Succeeded || status.Value is not { } value)
        {
            return Loading with
            {
                Headline = "限时运行模式不可用",
                Detail = "控制服务尚未返回紧急控制状态。",
            };
        }
        var hasDefaultProxy = outbounds.Value?.DefaultOutboundId is not null;
        var hasDefaultGroup = outbounds.Value?.DefaultOutboundGroupId is not null;
        var regularDetail = hasDefaultGroup
            ? "常规路由使用自动切换线路组；限时“全部代理”目前只接受单条默认代理。"
            : "紧急操作固定持续 5 分钟，并随快照分发到数据面。";
        if (value.PendingClearsOverride)
        {
            return new RuntimeOverridePanelState(
                "正在恢复常规策略",
                $"快照 v{value.PendingSnapshotVersion} 正等待系统组件确认；当前覆盖仍可能生效。",
                true,
                true,
                true,
                false,
                false,
                false);
        }
        if (value.Pending is { } pending && pending != value.Active)
        {
            var activeDetail = value.Active is { } currentActive
                ? $"当前{currentActive.ModeLabel}仍会持续到 {currentActive.ExpiryLabel}；"
                : string.Empty;
            return new RuntimeOverridePanelState(
                $"{pending.ModeLabel}等待确认",
                $"{activeDetail}新请求将在 {pending.ExpiryLabel} 前有效，但只有系统组件确认后才会生效。",
                true,
                value.Active is not null,
                true,
                false,
                false,
                false);
        }
        if (value.HasPendingMutation)
        {
            if (value.Active is { } carriedActive)
            {
                return new RuntimeOverridePanelState(
                    $"{carriedActive.ModeLabel}仍已生效",
                    $"当前模式将在 {carriedActive.ExpiryLabel} 自动到期；快照 v{value.PendingSnapshotVersion} 的其他配置正在等待确认。",
                    true,
                    true,
                    true,
                    false,
                    false,
                    false);
            }
            return new RuntimeOverridePanelState(
                "其他配置正在等待确认",
                $"快照 v{value.PendingSnapshotVersion} 尚未由系统组件确认，因此暂时不能提交限时运行请求。",
                true,
                false,
                true,
                false,
                false,
                false);
        }
        if (value.Active is { } active)
        {
            return new RuntimeOverridePanelState(
                $"{active.ModeLabel}已生效",
                $"将于 {active.ExpiryLabel} 自动恢复常规策略；无需保持桌面界面运行。",
                true,
                true,
                false,
                value.CanRequest,
                value.CanRequest && hasDefaultProxy,
                value.CanClear);
        }
        return new RuntimeOverridePanelState(
            "常规策略正在运行",
            regularDetail,
            true,
            false,
            false,
            value.CanRequest,
            value.CanRequest && hasDefaultProxy,
            false);
    }
}

public sealed record RuntimeOverrideConfirmation(
    RuntimeOverrideKind Kind,
    string Title,
    string Detail,
    string? OutboundId)
{
    public static RuntimeOverrideConfirmation Create(
        RuntimeOverrideKind kind,
        string? outboundId)
    {
        return kind switch
        {
            RuntimeOverrideKind.Paused => new(
                kind,
                "暂停 NonProxy 5 分钟？",
                "透明流量将交回当前系统路由；如果系统 VPN 仍开启，流量仍可能经过该 VPN。DNS 将使用 SYSTEM 路由。",
                null),
            RuntimeOverrideKind.Direct => new(
                kind,
                "全部直连 5 分钟？",
                "除系统保护流量外，连接将强制使用 NonProxy 的物理网卡隔离直连路径。",
                null),
            RuntimeOverrideKind.Proxy when !string.IsNullOrWhiteSpace(outboundId) => new(
                kind,
                "全部代理 5 分钟？",
                $"除系统保护流量外，连接将强制使用默认代理“{outboundId}”，代理失败时保持关闭。",
                outboundId),
            RuntimeOverrideKind.Proxy => throw new ControlServiceException(
                "NP_RUNTIME_OVERRIDE_PROXY_MISSING",
                "请先在“代理出口”中设置一个已验证的默认代理。"),
            _ => throw new ArgumentOutOfRangeException(nameof(kind)),
        };
    }
}
