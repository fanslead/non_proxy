using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Subscriptions;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class SubscriptionsViewTests
{
    [AvaloniaFact]
    public Task MacCompositionRendersSafeSubscriptionWorkflow()
    {
        return AssertSubscriptionWorkflowAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsCompositionRendersSafeSubscriptionWorkflow()
    {
        return AssertSubscriptionWorkflowAsync(PlatformKind.Windows, "Windows");
    }

    private static async Task AssertSubscriptionWorkflowAsync(
        PlatformKind platform,
        string displayName)
    {
        var subscriptionService = new VisibleSubscriptionService();
        using var services = TestPlatformServices.Create(
            platform,
            displayName,
            registrations => registrations.AddSingleton<ISubscriptionService>(
                subscriptionService));
        var viewModel = services.GetRequiredService<SubscriptionsViewModel>();
        var view = new SubscriptionsView { DataContext = viewModel };
        var window = new Window
        {
            // MainWindow's 960 px minimum leaves 720 px after the navigation rail.
            Width = 720,
            Height = 900,
            Content = view,
        };

        try
        {
            window.Show();
            await viewModel.RefreshCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            Assert.NotNull(view.FindControl<Button>("AddSubscriptionButton"));
            Assert.NotNull(view.FindControl<Border>("SubscriptionEditor"));
            Assert.NotNull(view.FindControl<ItemsControl>("SubscriptionItemsList"));
            var pageText = view
                .GetLogicalDescendants()
                .OfType<TextBlock>()
                .Select(text => text.Text)
                .Where(text => !string.IsNullOrWhiteSpace(text))
                .ToArray();
            Assert.Contains("订阅同步轨道", pageText);
            Assert.Contains("系统凭据库", pageText);
            Assert.Contains("安全拉取", pageText);
            Assert.Contains("节点出口", pageText);

            var item = Assert.Single(viewModel.Items);
            var list = view.FindControl<ItemsControl>("SubscriptionItemsList");
            var row = list?.ItemTemplate?.Build(item);
            Assert.NotNull(row);
            var buttons = row
                .GetLogicalDescendants()
                .OfType<Button>()
                .ToDictionary(button => button.Content as string ?? string.Empty);
            Assert.Equal("立即刷新此订阅", AutomationProperties.GetName(buttons["立即刷新"]));
            Assert.Equal("编辑订阅设置", AutomationProperties.GetName(buttons["设置"]));
            Assert.Single(
                buttons.Values,
                button => string.Equals(
                    AutomationProperties.GetName(button),
                    "启用或停用订阅",
                    StringComparison.Ordinal));
            Assert.Equal("请求删除订阅", AutomationProperties.GetName(buttons["删除"]));
            var rowText = row
                .GetLogicalDescendants()
                .OfType<TextBlock>()
                .Select(text => text.Text)
                .Where(text => !string.IsNullOrWhiteSpace(text))
                .ToArray();
            Assert.Contains("地址与 Token 不会回显", rowText);
            Assert.DoesNotContain(
                rowText,
                text => text!.Contains("provider.example", StringComparison.OrdinalIgnoreCase));

            viewModel.RequestDeleteCommand.Execute(item);
            var confirmationRow = list?.ItemTemplate?.Build(item);
            Assert.NotNull(confirmationRow);
            var confirmationText = confirmationRow
                .GetLogicalDescendants()
                .OfType<TextBlock>()
                .Select(text => text.Text)
                .ToArray();
            Assert.Contains("确认删除此订阅？", confirmationText);
            Assert.Equal(0, subscriptionService.DeleteCallCount);

            viewModel.OpenCreateCommand.Execute(null);
            Dispatcher.UIThread.RunJobs();
            Assert.True(view.FindControl<Border>("SubscriptionEditor")?.IsVisible);
            Assert.True(view.FindControl<TextBox>("SubscriptionEndpointInput")?.IsVisible);
            Assert.Empty(viewModel.EndpointUrl);
        }
        finally
        {
            window.Close();
        }
    }

    private sealed class VisibleSubscriptionService : ISubscriptionService
    {
        private static readonly SubscriptionListItem Item = new(
            "daily",
            "日常网络订阅",
            true,
            TimeSpan.FromHours(6),
            7,
            3,
            0,
            DateTimeOffset.UtcNow.AddHours(6),
            DateTimeOffset.UtcNow,
            DateTimeOffset.UtcNow,
            null,
            4);

        public int DeleteCallCount { get; private set; }

        public Task<SubscriptionCatalog> ListAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            DeleteCallCount = 0;
            return Task.FromResult(new SubscriptionCatalog([Item], DateTimeOffset.UtcNow));
        }

        public Task<SubscriptionMutation> SaveAsync(
            SubscriptionDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new SubscriptionMutation(
                true,
                "NP_SUBSCRIPTION_SAVED",
                "订阅已保存。",
                Item,
                false,
                []));
        }

        public Task<SubscriptionMutation> RefreshAsync(
            string sourceId,
            ulong expectedRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new SubscriptionMutation(
                true,
                "NP_SUBSCRIPTION_REFRESHED",
                "订阅已刷新。",
                Item,
                false,
                []));
        }

        public Task<SubscriptionDeletion> DeleteAsync(
            string sourceId,
            ulong expectedRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            DeleteCallCount++;
            return Task.FromResult(new SubscriptionDeletion(
                true,
                "NP_SUBSCRIPTION_DELETED",
                "订阅已删除。",
                sourceId,
                4,
                []));
        }
    }
}
