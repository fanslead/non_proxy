using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Networks;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class NetworkProfilesViewTests
{
    [AvaloniaFact]
    public Task MacViewExposesDetectAndTruthfulPublishActions()
    {
        return AssertViewAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsViewExposesDetectAndTruthfulPublishActions()
    {
        return AssertViewAsync(PlatformKind.Windows, "Windows");
    }

    private static async Task AssertViewAsync(
        PlatformKind platform,
        string displayName)
    {
        using var services = TestPlatformServices.Create(
            platform,
            displayName,
            collection =>
        {
            collection.AddSingleton<ICurrentNetworkEnvironment>(
                new UnavailableCurrentNetworkEnvironment());
            collection.AddSingleton<INetworkProfileService>(
                new EmptyNetworkProfileService());
        });
        var viewModel = services.GetRequiredService<NetworkProfilesViewModel>();
        var view = new NetworkProfilesView { DataContext = viewModel };
        var window = new Window
        {
            Width = 1_200,
            Height = 900,
            Content = view,
        };

        try
        {
            window.Show();
            Dispatcher.UIThread.RunJobs();
            var buttons = view.GetLogicalDescendants().OfType<Button>().ToArray();

            Assert.Contains(buttons, button =>
                AutomationProperties.GetName(button) == "检测当前物理网络");
            Assert.Contains(buttons, button =>
                AutomationProperties.GetName(button) == "保存并发布当前网络直连规则");
            Assert.NotNull(view.FindControl<ItemsControl>("NetworkProfileItemsList"));
            Assert.Contains(
                view.GetLogicalDescendants().OfType<TextBlock>(),
                text => text.Text?.Contains("不等于流量已经切换", StringComparison.Ordinal)
                    == true);
        }
        finally
        {
            window.Close();
        }
    }

    private sealed class EmptyNetworkProfileService : INetworkProfileService
    {
        public Task<NetworkProfileCatalog> GetCatalogAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(NetworkProfileCatalog.Empty);
        }

        public Task<NetworkProfileMutation> SaveAsync(
            NetworkProfileDraft draft,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }

        public Task<NetworkProfileMutation> DeleteAsync(
            string profileId,
            ulong expectedRevision,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }
    }
}
