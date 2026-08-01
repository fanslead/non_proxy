using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Features.Shell;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Dashboard;

public sealed partial class DashboardViewModel : LoadableViewModel
{
    private readonly IPlatformInformation _platformInformation;
    private readonly ISystemStatusService _statusService;
    private readonly IRuntimeOverrideService _runtimeOverrideService;
    private readonly ISystemComponentInstaller _componentInstaller;
    private readonly IOutboundService _outboundService;
    private readonly IAdapterManagementService _adapterService;
    private readonly IWorkspaceNavigator _navigator;
    private OptionalRead<OutboundCatalog> _lastOutbounds =
        OptionalRead<OutboundCatalog>.Unavailable;
    private OptionalRead<AdapterCatalog> _lastAdapters =
        OptionalRead<AdapterCatalog>.Unavailable;
    private ConnectionState? _liveConnectionState;

    [ObservableProperty]
    private DashboardState _state = DashboardState.Initial;

    [ObservableProperty]
    private DashboardSetupJourney _setup = DashboardSetupJourney.Initial;

    [ObservableProperty]
    private string? _operationMessage;

    [ObservableProperty]
    private bool _isUninstallConfirmationVisible;

    [ObservableProperty]
    private bool _requiresRestart;

    public DashboardViewModel(
        IPlatformInformation platformInformation,
        ISystemStatusService statusService,
        IRuntimeOverrideService runtimeOverrideService,
        ISystemComponentInstaller componentInstaller,
        IOutboundService outboundService,
        IAdapterManagementService adapterService,
        IWorkspaceNavigator navigator)
        : base("运行概览")
    {
        _platformInformation = platformInformation;
        _statusService = statusService;
        _runtimeOverrideService = runtimeOverrideService;
        _componentInstaller = componentInstaller;
        _outboundService = outboundService;
        _adapterService = adapterService;
        _navigator = navigator;
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
        NavigateSetupCommand = new RelayCommand<WorkspaceDestination?>(
            NavigateSetup);
        RefreshRuntimeCommand = new AsyncRelayCommand(
            RefreshRuntimeAsync,
            AsyncRelayCommandOptions.None);
        RequestPauseCommand = new RelayCommand(
            () => RequestRuntimeOverride(RuntimeOverrideKind.Paused));
        RequestDirectOverrideCommand = new RelayCommand(
            () => RequestRuntimeOverride(RuntimeOverrideKind.Direct));
        RequestProxyOverrideCommand = new RelayCommand(
            () => RequestRuntimeOverride(RuntimeOverrideKind.Proxy));
        CancelRuntimeOverrideCommand = new RelayCommand(
            () => EmergencyConfirmation = null);
        ConfirmRuntimeOverrideCommand = new AsyncRelayCommand(
            ConfirmRuntimeOverrideAsync,
            AsyncRelayCommandOptions.None);
        ClearRuntimeOverrideCommand = new AsyncRelayCommand(
            ClearRuntimeOverrideAsync,
            AsyncRelayCommandOptions.None);
    }

    public string PlatformLabel => _platformInformation.DisplayName;

    public IAsyncRelayCommand InstallComponentCommand { get; }

    public IAsyncRelayCommand OpenSystemSettingsCommand { get; }

    public IRelayCommand RequestUninstallCommand { get; }

    public IRelayCommand CancelUninstallCommand { get; }

    public IAsyncRelayCommand ConfirmUninstallCommand { get; }

    public IRelayCommand<WorkspaceDestination?> NavigateSetupCommand { get; }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var overviewTask = _statusService.GetOverviewAsync(cancellationToken);
        var outboundsTask = TryReadAsync(
            _outboundService.ListAsync,
            cancellationToken);
        var adaptersTask = TryReadAsync(
            _adapterService.ListAsync,
            cancellationToken);
        var runtimeOverrideTask = TryReadAsync(
            _runtimeOverrideService.GetStatusAsync,
            cancellationToken);
        await Task.WhenAll(
            overviewTask,
            outboundsTask,
            adaptersTask,
            runtimeOverrideTask);
        var overview = await overviewTask;
        _lastOutbounds = await outboundsTask;
        _lastAdapters = await adaptersTask;
        _lastRuntimeOverride = await runtimeOverrideTask;
        ApplyOverview(overview);
    }

    private void ApplyOverview(SystemOverview overview)
    {
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
        Setup = DashboardSetupJourney.Build(
            overview,
            _lastOutbounds,
            _lastAdapters);
        RuntimeOverride = RuntimeOverridePanelState.Build(
            _lastRuntimeOverride,
            _lastOutbounds);
        ScheduleRuntimeOverrideExpiryRefresh();
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

    private void NavigateSetup(WorkspaceDestination? destination)
    {
        if (destination is { } value)
        {
            _navigator.NavigateTo(value);
        }
    }

    private static async Task<OptionalRead<T>> TryReadAsync<T>(
        Func<CancellationToken, Task<T>> read,
        CancellationToken cancellationToken)
        where T : class
    {
        try
        {
            return OptionalRead<T>.Success(await read(cancellationToken));
        }
        catch (ControlServiceException exception)
            when (IsExpectedOptionalFailure(exception.Code))
        {
            return OptionalRead<T>.Unavailable;
        }
    }

    private static bool IsExpectedOptionalFailure(string code)
    {
        return code is "NP_CONTROL_UNAVAILABLE"
            or "NP_CONTROL_TIMEOUT"
            or "NP_CONTROL_SESSION_EXPIRED"
            or "NP_CONTROL_INTERRUPTED"
            or "NP_CONTROL_RPC_FAILED"
            or "NP_ADAPTER_UNAVAILABLE"
            or "NP_ADAPTER_TIMEOUT"
            or "NP_ADAPTER_SESSION_EXPIRED"
            or "NP_ADAPTER_SESSION_INVALID"
            or "NP_ADAPTER_INTERRUPTED"
            or "NP_ADAPTER_RPC_FAILED";
    }
}
