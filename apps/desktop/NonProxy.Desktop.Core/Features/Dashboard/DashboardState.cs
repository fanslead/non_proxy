namespace NonProxy.Desktop.Core.Features.Dashboard;

public sealed record DashboardState(
    string StatusHeadline,
    string StatusDetail,
    string ConnectionLabel,
    string ComponentLabel,
    string SnapshotLabel,
    int DirectApplicationCount,
    int DirectWebsiteCount,
    int RecentDecisionCount,
    bool HasRecentEvidence)
{
    public static DashboardState Initial { get; } = new(
        "正在读取系统状态",
        "NonProxy 正在检查控制服务和系统网络组件。",
        "正在连接",
        "正在检查",
        "尚无已激活快照",
        0,
        0,
        0,
        false);
}
