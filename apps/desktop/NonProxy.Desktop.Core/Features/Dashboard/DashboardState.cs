namespace NonProxy.Desktop.Core.Features.Dashboard;

public sealed record DashboardState(
    string StatusHeadline,
    string StatusDetail,
    int DirectApplicationCount,
    int DirectWebsiteCount,
    bool HasRecentEvidence)
{
    public static DashboardState Initial { get; } = new(
        "等待系统组件",
        "尚未安装或连接系统网络组件，当前不会接管任何网络流量。",
        0,
        0,
        false);
}
