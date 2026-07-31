using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Applications;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Tests;

public sealed class ApplicationsViewTests
{
    [AvaloniaFact]
    public Task MacCompositionRendersApplicationPicker()
    {
        return AssertApplicationPickerAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsCompositionRendersApplicationPicker()
    {
        return AssertApplicationPickerAsync(PlatformKind.Windows, "Windows");
    }

    private static async Task AssertApplicationPickerAsync(
        PlatformKind platform,
        string displayName)
    {
        using var services = TestPlatformServices.Create(
            platform,
            displayName,
            registrations => registrations.AddSingleton<IApplicationCatalog>(
                new VisibleApplicationCatalog()));
        var viewModel = services.GetRequiredService<ApplicationsViewModel>();
        var view = new ApplicationsView
        {
            DataContext = viewModel,
        };
        var window = new Window { Content = view };

        try
        {
            window.Show();
            await viewModel.RefreshCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            var choose = view.FindControl<Button>("ChooseApplicationButton");
            var search = view.FindControl<TextBox>("ApplicationSearchBox");
            var list = view.FindControl<ItemsControl>(
                "AvailableApplicationsList");

            Assert.True(choose?.IsEnabled);
            Assert.NotNull(search);
            Assert.Single(list?.Items.Cast<object>() ?? []);
        }
        finally
        {
            window.Close();
        }
    }

    private sealed class VisibleApplicationCatalog : IApplicationCatalog
    {
        private static readonly ApplicationCatalogEntry Application = new(
            "示例办公",
            "com.example.office",
            "TEAM123",
            "com.example.office",
            true);

        public Task<ApplicationCatalogSnapshot> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new ApplicationCatalogSnapshot(
                [Application],
                true,
                true,
                "可选择应用"));
        }

        public Task<ApplicationSelectionResult> ChooseAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new ApplicationSelectionResult(
                true,
                Application,
                "已选择应用"));
        }
    }
}
