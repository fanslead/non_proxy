using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Applications;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

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

    [AvaloniaFact]
    public async Task ApplicationActionReenablesAfterAsyncRefreshCompletes()
    {
        var policyService = new DelayedPolicyService();
        using var services = TestPlatformServices.Create(
            configure: registrations =>
            {
                registrations.AddSingleton<IPolicyService>(policyService);
                registrations.AddSingleton<IApplicationCatalog>(
                    new VisibleApplicationCatalog());
            });
        var viewModel = services.GetRequiredService<ApplicationsViewModel>();
        var view = new ApplicationsView
        {
            DataContext = viewModel,
        };
        var host = new StackPanel();
        host.Children.Add(view);
        var window = new Window { Content = host };
        Task? refreshTask = null;

        try
        {
            window.Show();
            refreshTask = viewModel.RefreshCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            Assert.True(viewModel.IsBusy);
            var progress = view.FindControl<ProgressBar>(
                "ApplicationCatalogProgress");
            Assert.True(progress?.IsVisible);
            Assert.True(progress?.IsIndeterminate);
            var item = Assert.Single(viewModel.AvailableApplications);
            var action = new Button
            {
                Command = viewModel.AddCommand,
                CommandParameter = item,
            };
            host.Children.Add(action);
            Dispatcher.UIThread.RunJobs();
            Assert.False(action.IsEffectivelyEnabled);

            policyService.CompleteCatalog();
            await refreshTask;
            Dispatcher.UIThread.RunJobs();

            Assert.False(viewModel.IsBusy);
            Assert.False(progress?.IsVisible);
            Assert.True(action.IsEffectivelyEnabled);
            Assert.Same(viewModel.AddCommand, action.Command);
            Assert.Same(item, action.CommandParameter);
        }
        finally
        {
            policyService.CompleteCatalog();
            if (refreshTask is not null)
            {
                await refreshTask;
            }
            window.Close();
        }
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

    private sealed class DelayedPolicyService : IPolicyService
    {
        private readonly TaskCompletionSource<PolicyCatalog> _catalog =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public Task<PolicyCatalog> GetCatalogAsync(
            CancellationToken cancellationToken)
        {
            return _catalog.Task.WaitAsync(cancellationToken);
        }

        public Task<ApplyResult> SaveAsync(
            PolicyDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(ApplyResult.Unavailable);
        }

        public Task<ApplyResult> DeleteAsync(
            string policyId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(ApplyResult.Unavailable);
        }

        public Task<ApplyResult> RollBackAsync(
            ulong snapshotVersion,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(ApplyResult.Unavailable);
        }

        public void CompleteCatalog()
        {
            _catalog.TrySetResult(PolicyCatalog.Empty);
        }
    }
}
