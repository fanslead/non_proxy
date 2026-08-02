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

    [AvaloniaFact]
    public Task MacCompositionRendersOutboundGroupLineStack()
    {
        return AssertOutboundGroupLineStackAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsCompositionRendersOutboundGroupLineStack()
    {
        return AssertOutboundGroupLineStackAsync(PlatformKind.Windows, "Windows");
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
            var explanatoryText = view
                .GetLogicalDescendants()
                .OfType<TextBlock>()
                .Select(text => text.Text)
                .Where(text => !string.IsNullOrWhiteSpace(text))
                .ToArray();
            Assert.Contains(
                explanatoryText,
                text => text!.Contains("ss://", StringComparison.Ordinal)
                    && text.Contains("加密密钥", StringComparison.Ordinal));

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

    private static async Task AssertOutboundGroupLineStackAsync(
        PlatformKind platform,
        string displayName)
    {
        using var services = TestPlatformServices.Create(
            platform,
            displayName,
            registrations =>
            {
                registrations.AddSingleton<IOutboundService>(
                    new VisibleGroupOutboundService());
                registrations.AddSingleton<IOutboundGroupService>(
                    new VisibleOutboundGroupService());
            });
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        var view = new OutboundsView { DataContext = viewModel };
        var window = new Window
        {
            Width = 1_200,
            Height = 1_000,
            Content = view,
        };

        try
        {
            window.Show();
            await viewModel.RefreshCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            var panel = view.FindControl<OutboundGroupsView>(
                "OutboundGroupsPanel");
            Assert.NotNull(panel);
            var create = panel.FindControl<Button>("CreateOutboundGroupButton");
            Assert.NotNull(create);
            Assert.True(create.IsEnabled);
            Assert.Equal(
                "新建自动切换线路组",
                AutomationProperties.GetName(create));

            var group = Assert.Single(viewModel.OutboundGroups.Groups);
            var list = panel.FindControl<ItemsControl>("OutboundGroupList");
            var row = list?.ItemTemplate?.Build(group);
            Assert.NotNull(row);
            var actions = row.GetLogicalDescendants().OfType<Button>().ToArray();
            Assert.Contains(actions, button =>
                string.Equals(button.Content as string, "设为默认", StringComparison.Ordinal));
            Assert.Contains(actions, button =>
                string.Equals(button.Content as string, "编辑顺序", StringComparison.Ordinal));

            viewModel.OutboundGroups.StartCreateCommand.Execute(null);
            foreach (var outbound in viewModel.OutboundGroups.AvailableOutbounds)
            {
                viewModel.OutboundGroups.AddMemberCommand.Execute(outbound);
            }
            Dispatcher.UIThread.RunJobs();

            var editor = panel.FindControl<Border>("OutboundGroupEditor");
            var stack = panel.FindControl<ItemsControl>(
                "OutboundGroupPriorityStack");
            Assert.True(editor?.IsVisible);
            Assert.Equal(2, stack?.ItemCount);
            Assert.Equal("01", viewModel.OutboundGroups.PriorityMembers[0].PositionLabel);
            Assert.Equal("02", viewModel.OutboundGroups.PriorityMembers[1].PositionLabel);
        }
        finally
        {
            window.Close();
        }
    }

    private class VisibleOutboundService : IOutboundService
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
            IsHandshakeVerified: true,
            CanVerifyExit: true);

        public virtual Task<OutboundCatalog> ListAsync(
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

    private sealed class VisibleGroupOutboundService : VisibleOutboundService
    {
        public override Task<OutboundCatalog> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundCatalog(
                [
                    new OutboundListItem(
                        "primary",
                        "Primary",
                        "SOCKS5",
                        "127.0.0.1:1080",
                        "代理握手可用",
                        TimeSpan.FromMilliseconds(20),
                        DateTimeOffset.UtcNow,
                        SupportsDefaultRoute: true,
                        IsHandshakeVerified: true),
                    new OutboundListItem(
                        "backup",
                        "Backup",
                        "Shadowsocks",
                        "proxy.example:8388",
                        "代理握手可用",
                        TimeSpan.FromMilliseconds(30),
                        DateTimeOffset.UtcNow,
                        SupportsDefaultRoute: true,
                        IsHandshakeVerified: true),
                ],
                4,
                ExitVerificationAvailable: true));
        }
    }

    private sealed class VisibleOutboundGroupService : IOutboundGroupService
    {
        public Task<OutboundGroupCatalog> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundGroupCatalog(
                [
                    new OutboundGroupListItem(
                        "automatic",
                        "Automatic",
                        ["primary", "backup"],
                        2),
                ],
                4));
        }

        public Task<OutboundGroupMutation> SaveAsync(
            OutboundGroupDraft draft,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }

        public Task<OutboundGroupDeletion> DeleteAsync(
            string groupId,
            ulong expectedRevision,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }

        public Task<ApplyResult> SetDefaultAsync(
            string groupId,
            ulong expectedRoutingRevision,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }
    }
}
