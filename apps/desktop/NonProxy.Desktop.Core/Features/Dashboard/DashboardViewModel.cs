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
    private ConnectionState? _liveConnectionState;

    [ObservableProperty]
    private DashboardState _state = DashboardState.Initial;

    [ObservableProperty]
    private string? _operationMessage;

    [ObservableProperty]
    private bool _isUninstallConfirmationVisible;

    [ObservableProperty]
    private bool _requiresRestart;

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
        OpenSystemSettingsCommand = new AsyncRelayCommand(
            OpenSystemSettingsAsync,
            AsyncRelayCommandOptions.None);
        RequestUninstallCommand = new RelayCommand(
            () => IsUninstallConfirmationVisible = true);
        CancelUninstallCommand = new RelayCommand(
            () => IsUninstallConfirmationVisible = false);
        ConfirmUninstallCommand = new AsyncRelayCommand(
            ConfirmUninstallAsync,
            AsyncRelayCommandOptions.None);
    }

    public string PlatformLabel => _platformInformation.DisplayName;

    public IAsyncRelayCommand InstallComponentCommand { get; }

    public IAsyncRelayCommand OpenSystemSettingsCommand { get; }

    public IRelayCommand RequestUninstallCommand { get; }

    public IRelayCommand CancelUninstallCommand { get; }

    public IAsyncRelayCommand ConfirmUninstallCommand { get; }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var overview = await _statusService.GetOverviewAsync(cancellationToken);
        var connection = overview.Connection == ConnectionState.Connected
            ? _liveConnectionState ?? overview.Connection
            : overview.Connection;
        State = new DashboardState(
            overview.Headline,
            overview.Detail,
            ToConnectionLabel(connection),
            overview.ComponentState,
            SnapshotLabel(overview),
            overview.DirectApplicationCount,
            overview.DirectWebsiteCount,
            overview.DirectNetworkCount,
            overview.RecentDecisionCount,
            overview.RecentDecisionCount > 0);
    }

    public void SetLiveConnectionState(ConnectionState state)
    {
        _liveConnectionState = state;
        State = State with
        {
            ConnectionLabel = ToConnectionLabel(state),
        };
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
                await ApplyComponentResultAsync(result, token);
            },
            cancellationToken);
    }

    private Task OpenSystemSettingsAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                var result = await _componentInstaller
                    .OpenSystemSettingsAsync(token);
                OperationMessage = result.Message;
                if (!result.Success)
                {
                    ErrorMessage = result.Message;
                }
            },
            cancellationToken);
    }

    private Task ConfirmUninstallAsync(CancellationToken cancellationToken)
    {
        IsUninstallConfirmationVisible = false;
        return RunOperationAsync(
            async token =>
            {
                var result = await _componentInstaller.UninstallAsync(token);
                await ApplyComponentResultAsync(result, token);
            },
            cancellationToken);
    }

    private async Task ApplyComponentResultAsync(
        InstallResult result,
        CancellationToken cancellationToken)
    {
        OperationMessage = result.Message;
        RequiresRestart = result.RequiresReboot;
        await LoadCoreAsync(cancellationToken);
        if (!result.Success
            && State.Component.Status
                != SystemComponentStatus.AwaitingApproval)
        {
            ErrorMessage = result.Message;
        }
    }

    internal static string ToConnectionLabel(ConnectionState state)
    {
        return state switch
        {
            ConnectionState.Connected => "状态同步正常",
            ConnectionState.Connecting => "正在连接控制服务",
            ConnectionState.Interrupted => "状态更新中断",
            _ => "控制服务未连接",
        };
    }

}
