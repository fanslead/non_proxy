using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Features.Dashboard;

public sealed record DashboardState(
    string StatusHeadline,
    string StatusDetail,
    string ConnectionLabel,
    SystemComponentState Component,
    string SnapshotLabel,
    int DirectApplicationCount,
    int DirectWebsiteCount,
    int DirectNetworkCount,
    int RecentDecisionCount,
    bool HasRecentEvidence)
{
    public string ComponentLabel => Component.Status switch
    {
        SystemComponentStatus.Installed => "系统组件已就绪",
        SystemComponentStatus.AwaitingApproval => "等待系统授权",
        SystemComponentStatus.Failed => "系统组件异常",
        SystemComponentStatus.Unavailable => "当前安装包不支持",
        SystemComponentStatus.Unknown => "系统组件状态未知",
        _ => "系统组件未安装",
    };

    public string ComponentActionLabel => Component.Status switch
    {
        SystemComponentStatus.AwaitingApproval => "我已允许，重新检查",
        SystemComponentStatus.Failed => "修复系统组件",
        SystemComponentStatus.Unknown => "重新检查系统组件",
        _ => "安装并启用",
    };

    public bool CanInstallOrRepair =>
        Component.Status is not SystemComponentStatus.Installed
            and not SystemComponentStatus.Unavailable;

    public bool CanUninstall =>
        Component.Status is SystemComponentStatus.Installed
            or SystemComponentStatus.AwaitingApproval
            or SystemComponentStatus.Failed;

    public bool HasComponentSteps => Component.Steps.Count > 0;

    public static DashboardState Initial { get; } = new(
        "正在读取系统状态",
        "NonProxy 正在检查控制服务和系统网络组件。",
        "正在连接",
        new SystemComponentState(
            SystemComponentStatus.Unknown,
            "正在检查系统组件。"),
        "尚无已激活快照",
        0,
        0,
        0,
        0,
        false);

    public IReadOnlyList<DashboardMetric> Metrics =>
    [
        new("直连应用", DirectApplicationCount, "应用级直连规则"),
        new("直连网站", DirectWebsiteCount, "网站与域名规则"),
        new("直连网络", DirectNetworkCount, "网络环境直连规则"),
        new("近期决策", RecentDecisionCount, "可核对的流量决策"),
    ];
}

public sealed record DashboardMetric(
    string Label,
    int Value,
    string Detail);
