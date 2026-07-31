using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Activity;
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

namespace NonProxy.Desktop.Core.Features.Shell;

public sealed partial class MainWindowViewModel : ObservableObject
{
    private bool _isInitialized;

    [ObservableProperty]
    private NavigationItemViewModel? _selectedNavigation;

    [ObservableProperty]
    private IPageViewModel _currentPage;

    public MainWindowViewModel(
        DashboardViewModel dashboard,
        PoliciesViewModel policies,
        ApplicationsViewModel applications,
        WebsitesViewModel websites,
        NetworkProfilesViewModel networks,
        OutboundsViewModel outbounds,
        LearningViewModel learning,
        ActivityViewModel activity,
        DiagnosticsViewModel diagnostics,
        SettingsViewModel settings,
        IPlatformInformation platformInformation)
    {
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

    private async Task InitializeAsync(CancellationToken cancellationToken)
    {
        if (_isInitialized)
        {
            return;
        }

        _isInitialized = true;
        await CurrentPage.RefreshCommand.ExecuteAsync(null);
        cancellationToken.ThrowIfCancellationRequested();
    }
}
