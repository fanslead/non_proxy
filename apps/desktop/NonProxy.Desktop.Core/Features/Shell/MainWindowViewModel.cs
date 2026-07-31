using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Activity;
using NonProxy.Desktop.Core.Features.Adapters;
using NonProxy.Desktop.Core.Features.Applications;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Features.Diagnostics;
using NonProxy.Desktop.Core.Features.Learning;
using NonProxy.Desktop.Core.Features.Networks;
using NonProxy.Desktop.Core.Features.Outbounds;
using NonProxy.Desktop.Core.Features.Policies;
using NonProxy.Desktop.Core.Features.Settings;
using NonProxy.Desktop.Core.Features.Websites;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Events;

namespace NonProxy.Desktop.Core.Features.Shell;

public sealed partial class MainWindowViewModel : ObservableObject, IDisposable
{
    private const int RefreshDashboard = 1 << 0;
    private const int RefreshPolicies = 1 << 1;
    private const int RefreshActivity = 1 << 2;
    private const int RefreshDiagnostics = 1 << 3;
    private const int RefreshCurrent = 1 << 4;
    private static readonly TimeSpan RefreshDebounce = TimeSpan.FromMilliseconds(200);

    private bool _isInitialized;
    private readonly IControlEventMonitor _eventMonitor;
    private readonly CancellationTokenSource _lifetime = new();
    private readonly CancellationToken _lifetimeToken;
    private int _pendingRefresh;
    private int _refreshGeneration;
    private bool _disposed;

    [ObservableProperty]
    private NavigationItemViewModel? _selectedNavigation;

    [ObservableProperty]
    private IPageViewModel _currentPage;

    [ObservableProperty]
    private string _liveStatusLabel = "状态同步：尚未启动";

    public MainWindowViewModel(
        DashboardViewModel dashboard,
        PoliciesViewModel policies,
        ApplicationsViewModel applications,
        WebsitesViewModel websites,
        NetworkProfilesViewModel networks,
        OutboundsViewModel outbounds,
        AdaptersViewModel adapters,
        LearningViewModel learning,
        ActivityViewModel activity,
        DiagnosticsViewModel diagnostics,
        SettingsViewModel settings,
        IPlatformInformation platformInformation,
        IControlEventMonitor eventMonitor)
    {
        _eventMonitor = eventMonitor;
        _lifetimeToken = _lifetime.Token;
        Dashboard = dashboard;
        PlatformLabel = platformInformation.DisplayName;
        NavigationItems =
        [
            new("运行概览", "⌂", dashboard),
            new("全部规则", "≡", policies),
            new("应用直连", "▣", applications),
            new("网站直连", "◎", websites),
            new("网络环境", "⌁", networks),
            new("网络出口", "⇄", outbounds),
            new("客户端协同", "⟷", adapters),
            new("智能学习", "✦", learning),
            new("活动记录", "◷", activity),
            new("诊断", "◇", diagnostics),
            new("设置", "⚙", settings),
        ];

        _currentPage = dashboard;
        _selectedNavigation = NavigationItems[0];
        InitializeCommand = new AsyncRelayCommand(InitializeAsync);
    }

    public DashboardViewModel Dashboard { get; }

    public string PlatformLabel { get; }

    public IReadOnlyList<NavigationItemViewModel> NavigationItems { get; }

    public IAsyncRelayCommand InitializeCommand { get; }

    partial void OnSelectedNavigationChanged(NavigationItemViewModel? value)
    {
        if (value is null || ReferenceEquals(CurrentPage, value.Page))
        {
            return;
        }

        CurrentPage = value.Page;
        if (_isInitialized)
        {
            _ = value.Page.RefreshCommand.ExecuteAsync(null);
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _eventMonitor.StateChanged -= OnMonitorStateChanged;
        _eventMonitor.EventReceived -= OnControlEventReceived;
        _lifetime.Cancel();
        _lifetime.Dispose();
    }

    private async Task InitializeAsync(CancellationToken cancellationToken)
    {
        if (_isInitialized)
        {
            return;
        }

        _isInitialized = true;
        await CurrentPage.RefreshCommand.ExecuteAsync(null);
        cancellationToken.ThrowIfCancellationRequested();
        if (_disposed)
        {
            return;
        }

        _eventMonitor.StateChanged += OnMonitorStateChanged;
        _eventMonitor.EventReceived += OnControlEventReceived;
        _ = RunMonitorAsync(_lifetimeToken);
    }

    private async Task RunMonitorAsync(CancellationToken cancellationToken)
    {
        try
        {
            await _eventMonitor.RunAsync(cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
    }

    private void OnMonitorStateChanged(ConnectionState state)
    {
        Dispatcher.UIThread.Post(() =>
        {
            if (_disposed)
            {
                return;
            }

            LiveStatusLabel = state switch
            {
                ConnectionState.Connecting => "状态同步：正在连接",
                ConnectionState.Connected => "状态同步：实时",
                ConnectionState.Interrupted => "状态同步：已中断，正在重连",
                _ => "状态同步：已停止",
            };
            Dashboard.SetLiveConnectionState(state);
            if (state == ConnectionState.Connected)
            {
                QueueRefresh(RefreshDashboard | RefreshCurrent);
            }
        });
    }

    private void OnControlEventReceived(ControlEventNotification notification)
    {
        if (_disposed)
        {
            return;
        }

        var impact = notification.Kind switch
        {
            ControlEventKind.Snapshot => RefreshDashboard | RefreshPolicies,
            ControlEventKind.Decision => RefreshDashboard | RefreshActivity,
            ControlEventKind.SystemState or ControlEventKind.ComponentHealth =>
                RefreshDashboard | RefreshDiagnostics,
            ControlEventKind.Unknown => RefreshDashboard,
            _ => 0,
        };
        if (impact != 0)
        {
            QueueRefresh(impact);
        }
    }

    private void QueueRefresh(int impact)
    {
        if (_disposed)
        {
            return;
        }

        Interlocked.Or(ref _pendingRefresh, impact);
        var generation = Interlocked.Increment(ref _refreshGeneration);
        _ = FlushRefreshAsync(generation, _lifetimeToken);
    }

    private async Task FlushRefreshAsync(
        int generation,
        CancellationToken cancellationToken)
    {
        try
        {
            await Task.Delay(RefreshDebounce, cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            return;
        }

        if (generation != Volatile.Read(ref _refreshGeneration))
        {
            return;
        }

        var impact = Interlocked.Exchange(ref _pendingRefresh, 0);
        Dispatcher.UIThread.Post(() =>
        {
            if (!_disposed)
            {
                _ = RefreshImpactedPagesAsync(impact);
            }
        });
    }

    private async Task RefreshImpactedPagesAsync(int impact)
    {
        if (_disposed)
        {
            return;
        }

        if ((impact & RefreshDashboard) != 0)
        {
            await Dashboard.RefreshCommand.ExecuteAsync(null);
        }

        if (_disposed || ReferenceEquals(CurrentPage, Dashboard))
        {
            return;
        }

        var refreshCurrent = (impact & RefreshCurrent) != 0
            || (impact & RefreshPolicies) != 0 && CurrentPage is
                PoliciesViewModel or ApplicationsViewModel or WebsitesViewModel
                or NetworkProfilesViewModel or OutboundsViewModel or AdaptersViewModel
            || (impact & RefreshActivity) != 0 && CurrentPage is ActivityViewModel
            || (impact & RefreshDiagnostics) != 0 && CurrentPage is DiagnosticsViewModel;
        if (refreshCurrent)
        {
            await CurrentPage.RefreshCommand.ExecuteAsync(null);
        }
    }
}
