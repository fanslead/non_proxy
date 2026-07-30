using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Dashboard;

public sealed partial class DashboardViewModel : LoadableViewModel
{
    private readonly IPlatformInformation _platformInformation;
    private readonly ISystemStatusService _statusService;
    private readonly ISystemComponentInstaller _componentInstaller;

    [ObservableProperty]
    private DashboardState _state = DashboardState.Initial;

    [ObservableProperty]
    private string? _operationMessage;

    public DashboardViewModel(
        IPlatformInformation platformInformation,
        ISystemStatusService statusService,
        ISystemComponentInstaller componentInstaller)
        : base("运行概览")
    {
        _platformInformation = platformInformation;
        _statusService = statusService;
        _componentInstaller = componentInstaller;
        InstallComponentCommand = new AsyncRelayCommand(
            InstallComponentAsync,
            AsyncRelayCommandOptions.None);
    }

    public string PlatformLabel => _platformInformation.DisplayName;

    public IAsyncRelayCommand InstallComponentCommand { get; }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var overview = await _statusService.GetOverviewAsync(cancellationToken);
        State = new DashboardState(
            overview.Headline,
            overview.Detail,
            ToConnectionLabel(overview.Connection),
            ToComponentLabel(overview.Component),
            SnapshotLabel(overview),
            overview.DirectApplicationCount,
            overview.DirectWebsiteCount,
            overview.RecentDecisionCount,
            overview.RecentDecisionCount > 0);
    }

    private static string SnapshotLabel(SystemOverview overview)
    {
        return (overview.ActiveSnapshotVersion, overview.PendingSnapshotVersion) switch
        {
            ({ } active, { } pending) => $"已激活 v{active}，等待确认 v{pending}",
            ({ } active, null) => $"已激活快照 v{active}",
            (null, { } pending) => $"快照 v{pending} 等待系统组件确认",
            _ => "尚无已激活快照",
        };
    }

    private Task InstallComponentAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                var result = await _componentInstaller.InstallAsync(token);
                OperationMessage = result.Message;
                if (result.Success)
                {
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
    }

    private static string ToConnectionLabel(ConnectionState state)
    {
        return state switch
        {
            ConnectionState.Connected => "状态同步正常",
            ConnectionState.Connecting => "正在连接控制服务",
            ConnectionState.Interrupted => "状态更新中断",
            _ => "控制服务未连接",
        };
    }

    private static string ToComponentLabel(SystemComponentStatus status)
    {
        return status switch
        {
            SystemComponentStatus.Installed => "系统组件已就绪",
            SystemComponentStatus.AwaitingApproval => "等待系统授权",
            SystemComponentStatus.Failed => "系统组件异常",
            SystemComponentStatus.Unavailable => "当前安装包不含系统组件",
            _ => "系统组件未安装",
        };
    }
}
