using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Policies;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class PoliciesRestoreViewTests
{
    [AvaloniaFact]
    public Task MacCompositionRendersExplicitRestoreConfirmation()
    {
        return AssertRestoreViewAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsCompositionRendersExplicitRestoreConfirmation()
    {
        return AssertRestoreViewAsync(PlatformKind.Windows, "Windows");
    }

    private static async Task AssertRestoreViewAsync(
        PlatformKind platform,
        string displayName)
    {
        var policies = new PoliciesRestoreViewModelTests.RestorePolicyService();
        using var services = TestPlatformServices.Create(
            platform,
            displayName,
            registrations => registrations.AddSingleton<IPolicyService>(policies));
        var viewModel = services.GetRequiredService<PoliciesViewModel>();
        var view = new PoliciesView { DataContext = viewModel };
        var window = new Window { Width = 1_100, Height = 800, Content = view };

        try
        {
            window.Show();
            await viewModel.RefreshCommand.ExecuteAsync(null);
            viewModel.RequestRestorePreviousCommand.Execute(null);
            Dispatcher.UIThread.RunJobs();

            var request = Assert.IsType<Button>(
                view.FindControl<Button>("RequestRestorePreviousButton"));
            var confirmation = Assert.IsType<Border>(
                view.FindControl<Border>("RestorePreviousConfirmation"));
            Assert.Equal(
                "恢复上一个已生效配置",
                AutomationProperties.GetName(request));
            Assert.True(request.IsEnabled);
            Assert.True(confirmation.IsVisible);
        }
        finally
        {
            window.Close();
        }
    }
}
