using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Outbounds;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class OutboundsViewTests
{
    [AvaloniaFact]
    public Task MacCompositionRendersOutboundTestAction()
    {
        return AssertOutboundTestActionAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsCompositionRendersOutboundTestAction()
    {
        return AssertOutboundTestActionAsync(PlatformKind.Windows, "Windows");
    }

    private static async Task AssertOutboundTestActionAsync(
        PlatformKind platform,
        string displayName)
    {
        using var services = TestPlatformServices.Create(
            platform,
            displayName,
            registrations => registrations.AddSingleton<IOutboundService>(
                new VisibleOutboundService()));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        var view = new OutboundsView
        {
            DataContext = viewModel,
        };
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
            Assert.Single(viewModel.Items);

            var list = view.FindControl<ItemsControl>("OutboundItemsList");
            var row = list?.ItemTemplate?.Build(viewModel.Items[0]);
            Assert.NotNull(row);
            var buttons = row
                .GetLogicalDescendants()
                .OfType<Button>()
                .ToArray();
            var action = buttons
                .SingleOrDefault(button =>
                    string.Equals(
                        button.Content as string,
                        "测试",
                        StringComparison.Ordinal));

            Assert.True(
                action is not null,
                $"未找到测试按钮。已渲染按钮：{string.Join(", ", buttons.Select(button => button.Content))}");
            Assert.True(action.IsEnabled);
            Assert.Equal("测试代理握手", AutomationProperties.GetName(action));
        }
        finally
        {
            window.Close();
        }
    }

    private sealed class VisibleOutboundService : IOutboundService
    {
        private static readonly OutboundListItem Outbound = new(
            "office",
            "Office proxy",
            "SOCKS5",
            "127.0.0.1:1080",
            "未验证",
            null,
            null);

        public Task<IReadOnlyList<OutboundListItem>> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult<IReadOnlyList<OutboundListItem>>([Outbound]);
        }

        public Task<OutboundTestResult> TestAsync(
            string outboundId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundTestResult(
                outboundId,
                true,
                "代理握手可用",
                TimeSpan.FromMilliseconds(25),
                DateTimeOffset.UtcNow,
                "代理握手成功。"));
        }

        public Task<OutboundImportResult> ImportAsync(
            OutboundImportDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundImportResult(
                "unused",
                [Outbound],
                []));
        }
    }
}
