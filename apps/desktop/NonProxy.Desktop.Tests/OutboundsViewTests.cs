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
                        "测试握手",
                        StringComparison.Ordinal));

            Assert.True(
                action is not null,
                $"未找到测试按钮。已渲染按钮：{string.Join(", ", buttons.Select(button => button.Content))}");
            Assert.True(action.IsEnabled);
            Assert.Equal("测试代理握手", AutomationProperties.GetName(action));
            var exitAction = buttons.SingleOrDefault(button =>
                string.Equals(
                    button.Content as string,
                    "验证出口",
                    StringComparison.Ordinal));
            Assert.NotNull(exitAction);
            Assert.True(exitAction.IsEnabled);
            Assert.Equal(
                "验证代理公网出口",
                AutomationProperties.GetName(exitAction));
            var defaultAction = buttons.SingleOrDefault(button =>
                string.Equals(
                    button.Content as string,
                    "设为默认",
                    StringComparison.Ordinal));
            Assert.NotNull(defaultAction);
            Assert.True(defaultAction.IsEnabled);
            Assert.Equal(
                "设为默认代理",
                AutomationProperties.GetName(defaultAction));
            var directAction = view
                .GetLogicalDescendants()
                .OfType<Button>()
                .SingleOrDefault(button =>
                    string.Equals(
                        button.Content as string,
                        "验证直连出口",
                        StringComparison.Ordinal));
            Assert.NotNull(directAction);
            Assert.True(directAction.IsEnabled);
            Assert.Equal(
                "验证物理直连公网出口",
                AutomationProperties.GetName(directAction));
            var discoveryAction = view.FindControl<Button>(
                "DiscoverLocalProxyButton");
            Assert.NotNull(discoveryAction);
            Assert.True(discoveryAction.IsEnabled);
            Assert.Equal(
                "自动发现当前系统 SOCKS 或 HTTP 代理",
                AutomationProperties.GetName(discoveryAction));

            var uriInput = view.FindControl<TextBox>("ProxyUriImportText");
            var uriPreview = view.FindControl<Border>("ProxyUriImportPreview");
            Assert.NotNull(uriInput);
            Assert.False(uriPreview?.IsVisible);
            viewModel.UriImportText =
                "socks5://alice:private@proxy.example:1080#office";
            await viewModel.PreviewUriImportCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();
            Assert.True(uriPreview?.IsVisible);
            Assert.Single(viewModel.UriImportPreview);
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
            null,
            SupportsDefaultRoute: true,
            CanVerifyExit: true);

        public Task<OutboundCatalog> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundCatalog(
                [Outbound],
                1,
                ExitVerificationAvailable: true));
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

        public Task<ExitVerificationResult> VerifyExitAsync(
            string? outboundId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new ExitVerificationResult(
                true,
                "NP_EXIT_PROBE_VERIFIED",
                "公网出口已签名验证。"));
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

        public Task<OutboundImportResult> PreviewUriListAsync(
            string uriList,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundImportResult(
                "preview",
                [Outbound],
                []));
        }

        public Task<OutboundImportResult> ImportUriListAsync(
            string uriList,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundImportResult(
                "import",
                [Outbound],
                []));
        }

        public Task<ApplyResult> SetDefaultAsync(
            string outboundId,
            ulong expectedRoutingRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new ApplyResult(
                true,
                false,
                "NP_SNAPSHOT_PENDING_ACK",
                "等待系统组件确认。",
                1));
        }

        public Task<ApplyResult> SetDirectAsync(
            ulong expectedRoutingRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new ApplyResult(
                true,
                false,
                "NP_SNAPSHOT_PENDING_ACK",
                "等待系统组件确认。",
                1));
        }
    }
}
