using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Features.Shell;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Views;

namespace NonProxy.Desktop.Tests;

public sealed class DashboardViewTests
{
    [AvaloniaFact]
    public Task MacCompositionRendersTruthfulSetupJourney()
    {
        return AssertSetupJourneyAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsCompositionRendersTruthfulSetupJourney()
    {
        return AssertSetupJourneyAsync(PlatformKind.Windows, "Windows");
    }

    [AvaloniaFact]
    public void InitialStateRendersStatusHeadline()
    {
        using var services = TestPlatformServices.Create();
        var view = new DashboardView
        {
            DataContext = services.GetRequiredService<DashboardViewModel>(),
        };
        var window = new Window
        {
            Content = view,
        };

        try
        {
            window.Show();

            var headline = view.FindControl<TextBlock>("StatusHeadline");
            Assert.NotNull(headline);
            Assert.Equal("正在读取系统状态", headline.Text);
        }
        finally
        {
            window.Close();
        }
    }

    [AvaloniaFact]
    public async Task ApprovalRecoveryControlsAreRenderedFromRealState()
    {
        var installer = new RecordingSystemComponentInstaller
        {
            State = new SystemComponentState(
                SystemComponentStatus.AwaitingApproval,
                "等待授权",
                steps:
                [
                    new(
                        "gateway",
                        "后台服务",
                        SystemComponentStepStatus.AwaitingApproval,
                        "等待允许"),
                ],
                canOpenSystemSettings: true),
        };
        using var services = TestPlatformServices.Create(
            configure: registrations =>
                registrations.AddSingleton<ISystemComponentInstaller>(
                    installer));
        var viewModel = services.GetRequiredService<DashboardViewModel>();
        var view = new DashboardView
        {
            DataContext = viewModel,
        };
        var window = new Window
        {
            Content = view,
        };

        try
        {
            window.Show();
            await viewModel.RefreshCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            var install = view.FindControl<Button>("InstallComponentButton");
            var settings = view.FindControl<Button>("OpenSystemSettingsButton");
            var uninstall = view.FindControl<Button>("RequestUninstallButton");

            Assert.Equal("我已允许，重新检查", install?.Content);
            Assert.True(settings?.IsVisible);
            Assert.True(uninstall?.IsVisible);
        }
        finally
        {
            window.Close();
        }
    }

    [AvaloniaFact]
    public void CompositionRootResolvesBoundMainWindow()
    {
        using var services = TestPlatformServices.Create();

        var window = services.GetRequiredService<MainWindow>();

        try
        {
            Assert.Same(
                services.GetRequiredService<MainWindowViewModel>(),
                window.DataContext);
        }
        finally
        {
            window.Close();
        }
    }

    private static async Task AssertSetupJourneyAsync(
        PlatformKind platform,
        string displayName)
    {
        using var services = TestPlatformServices.Create(platform, displayName);
        var viewModel = services.GetRequiredService<DashboardViewModel>();
        var view = new DashboardView { DataContext = viewModel };
        var window = new Window
        {
            Width = 1_200,
            Height = 900,
            Content = view,
        };

        try
        {
            window.Show();
            await viewModel.RefreshCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            var journey = Assert.IsType<DashboardSetupJourneyView>(
                view.FindControl<DashboardSetupJourneyView>("SetupJourney"));
            var gateway = Assert.IsType<Button>(
                journey.FindControl<Button>("OpenGatewaySetupButton"));
            var adapter = Assert.IsType<Button>(
                journey.FindControl<Button>("OpenAdapterSetupButton"));
            Assert.Equal(
                "继续完整网关设置",
                AutomationProperties.GetName(gateway));
            Assert.Equal(
                "继续第三方客户端协同设置",
                AutomationProperties.GetName(adapter));
            Assert.True(adapter.IsVisible);
            Assert.Contains(
                "不会猜测",
                viewModel.Setup.Adapter.Detail,
                StringComparison.Ordinal);
        }
        finally
        {
            window.Close();
        }
    }
}
