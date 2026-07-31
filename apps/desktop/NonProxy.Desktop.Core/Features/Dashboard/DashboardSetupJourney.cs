using NonProxy.Adapter.V1;
using NonProxy.Desktop.Core.Features.Shell;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Dashboard;

public enum DashboardSetupTone
{
    Ready,
    Attention,
    Pending,
    Unavailable,
}

public sealed record DashboardSetupPath(
    string Eyebrow,
    string Title,
    string Status,
    string Detail,
    string ActionLabel,
    WorkspaceDestination? Destination,
    DashboardSetupTone Tone)
{
    public bool CanNavigate => Destination is not null;

    public bool IsReady => Tone == DashboardSetupTone.Ready;

    public bool IsAttention => Tone == DashboardSetupTone.Attention;

    public bool IsPending => Tone == DashboardSetupTone.Pending;

    public bool IsUnavailable => Tone == DashboardSetupTone.Unavailable;
}

public sealed record DashboardSetupJourney(
    string Summary,
    DashboardSetupPath Gateway,
    DashboardSetupPath Adapter)
{
    public static DashboardSetupJourney Initial { get; } = new(
        "正在核对两种接入方式",
        PendingPath("完整网关", "正在读取系统组件、默认代理和活动快照。"),
        PendingPath("客户端协同", "正在读取第三方客户端登记状态。"));

    internal static DashboardSetupJourney Build(
        SystemOverview overview,
        OptionalRead<OutboundCatalog> outbounds,
        OptionalRead<AdapterCatalog> adapters)
    {
        var gateway = BuildGateway(overview, outbounds);
        var adapter = BuildAdapter(adapters);
        var summary = gateway.IsReady
            ? "完整网关基础链路已就绪；继续用真实活动记录核对每条路径。"
            : adapter.IsPending
                ? "已登记第三方客户端；同步成功仍需独立的真实路径证据。"
                : "选择一种接入方式；两种方式不会同时宣称接管同一条流量。";
        return new DashboardSetupJourney(summary, gateway, adapter);
    }

    private static DashboardSetupPath BuildGateway(
        SystemOverview overview,
        OptionalRead<OutboundCatalog> outbounds)
    {
        if (overview.Component == SystemComponentStatus.Unavailable)
        {
            return new DashboardSetupPath(
                "效果最完整",
                "完整网关",
                "当前安装不可用",
                "此安装不能启用系统网络组件；不会把仓库构建当成真实接管。",
                string.Empty,
                null,
                DashboardSetupTone.Unavailable);
        }
        if (overview.Component != SystemComponentStatus.Installed)
        {
            return new DashboardSetupPath(
                "效果最完整",
                "完整网关",
                overview.Component == SystemComponentStatus.AwaitingApproval
                    ? "等待系统授权"
                    : "需要系统组件",
                "先在本页完成系统组件安装、授权或修复，再配置默认代理。",
                string.Empty,
                null,
                DashboardSetupTone.Attention);
        }
        if (!outbounds.Succeeded || outbounds.Value is null)
        {
            return new DashboardSetupPath(
                "效果最完整",
                "完整网关",
                "出口状态暂不可读",
                "系统组件已就绪，但当前无法确认默认路由；打开网络出口重试。",
                "查看网络出口",
                WorkspaceDestination.Outbounds,
                DashboardSetupTone.Unavailable);
        }

        var catalog = outbounds.Value;
        var defaultOutbound = catalog.DefaultOutboundId is { } id
            ? catalog.Items.SingleOrDefault(item =>
                string.Equals(item.Id, id, StringComparison.Ordinal))
            : null;
        if (defaultOutbound is null)
        {
            return new DashboardSetupPath(
                "效果最完整",
                "完整网关",
                "需要默认代理",
                "当前未命中规则时仍默认直连；先选择并验证一个代理出口。",
                "配置默认代理",
                WorkspaceDestination.Outbounds,
                DashboardSetupTone.Attention);
        }

        var directRuleCount = overview.DirectApplicationCount
            + overview.DirectWebsiteCount
            + overview.DirectNetworkCount;
        if (directRuleCount == 0)
        {
            return new DashboardSetupPath(
                "效果最完整",
                "完整网关",
                "还没有直连目标",
                "默认代理已配置；添加第一个应用或网站后才会形成分流策略。",
                "添加直连应用",
                WorkspaceDestination.Applications,
                DashboardSetupTone.Attention);
        }
        if (overview.ActiveDirectRuleCount == 0)
        {
            return new DashboardSetupPath(
                "效果最完整",
                "完整网关",
                "直连规则尚未激活",
                $"已保存 {directRuleCount} 条直连目标，但活动快照尚未包含它们。",
                "检查规则状态",
                WorkspaceDestination.Policies,
                DashboardSetupTone.Pending);
        }
        if (overview.PendingSnapshotVersion is not null || !overview.DataPlaneEnabled)
        {
            return new DashboardSetupPath(
                "效果最完整",
                "完整网关",
                "等待数据面确认",
                $"已有 {overview.ActiveDirectRuleCount} 条活动直连规则，但当前快照尚未被系统组件完整确认。",
                "查看活动记录",
                WorkspaceDestination.Activity,
                DashboardSetupTone.Pending);
        }

        return new DashboardSetupPath(
            "效果最完整",
            "完整网关",
            "基础链路已就绪",
            $"默认代理和 {overview.ActiveDirectRuleCount} 条直连规则已激活；近期决策 {overview.RecentDecisionCount} 条。",
            "核对真实路径",
            WorkspaceDestination.Activity,
            DashboardSetupTone.Ready);
    }

    private static DashboardSetupPath BuildAdapter(
        OptionalRead<AdapterCatalog> adapters)
    {
        if (!adapters.Succeeded || adapters.Value is null)
        {
            return new DashboardSetupPath(
                "兼容现有客户端",
                "客户端协同",
                "Adapter 宿主不可用",
                "无法读取隔离的客户端登记目录；不会猜测本机端口或私有配置。",
                "查看客户端协同",
                WorkspaceDestination.Adapters,
                DashboardSetupTone.Unavailable);
        }

        var usableCount = adapters.Value.Items.Count(item =>
            item.State is AdapterState.Available or AdapterState.Ready);
        if (usableCount == 0)
        {
            return new DashboardSetupPath(
                "兼容现有客户端",
                "客户端协同",
                "尚未登记客户端",
                "显式选择 Surge、Clash/Mihomo 或 sing-box 及其当前主配置。",
                "登记第三方客户端",
                WorkspaceDestination.Adapters,
                DashboardSetupTone.Attention);
        }

        return new DashboardSetupPath(
            "兼容现有客户端",
            "客户端协同",
            $"已登记 {usableCount} 个客户端",
            "继续同步并核对候选、配置和真实路径三段证据；登记本身不代表已直连。",
            "同步并查看证据",
            WorkspaceDestination.Adapters,
            DashboardSetupTone.Pending);
    }

    private static DashboardSetupPath PendingPath(string title, string detail)
    {
        return new DashboardSetupPath(
            title == "完整网关" ? "效果最完整" : "兼容现有客户端",
            title,
            "正在读取",
            detail,
            string.Empty,
            null,
            DashboardSetupTone.Pending);
    }
}

internal sealed record OptionalRead<T>(bool Succeeded, T? Value)
    where T : class
{
    public static OptionalRead<T> Success(T value) => new(true, value);

    public static OptionalRead<T> Unavailable { get; } = new(false, null);
}
