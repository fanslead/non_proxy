using System.Globalization;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Outbounds;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class OutboundsViewModelTests
{
    [Fact]
    public async Task StructuredImportClearsPasswordAndRefreshesList()
    {
        var outboundService = new RecordingOutboundService();
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        viewModel.OutboundId = "office";
        viewModel.Host = "127.0.0.1";
        viewModel.Port = "1080";
        viewModel.Username = "alice";
        viewModel.Password = "private";

        await viewModel.ImportCommand.ExecuteAsync(null);

        Assert.NotNull(outboundService.LastDraft);
        Assert.Equal("office", outboundService.LastDraft.Id);
        Assert.Equal("private", outboundService.LastDraft.Password);
        Assert.Empty(viewModel.Password);
        Assert.Equal("office", Assert.Single(viewModel.Items).Id);
        Assert.Contains("安全保存", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task InvalidPortNeverReachesOutboundService()
    {
        var outboundService = new RecordingOutboundService();
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        viewModel.Port = "70000";

        await viewModel.ImportCommand.ExecuteAsync(null);

        Assert.Null(outboundService.LastDraft);
        Assert.Contains("65535", viewModel.ValidationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task TestCommandUpdatesOnlySelectedOutbound()
    {
        var outboundService = new RecordingOutboundService();
        outboundService.Seed(
            new OutboundListItem(
                "office",
                "A Office",
                "SOCKS5",
                "127.0.0.1:1080",
                "未验证",
                null,
                null),
            new OutboundListItem(
                "backup",
                "B Backup",
                "HTTP CONNECT",
                "127.0.0.1:8080",
                "未验证",
                null,
                null));
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);
        var selected = viewModel.Items[0];

        await viewModel.TestCommand.ExecuteAsync(selected);

        Assert.Equal("office", outboundService.LastTestedOutboundId);
        Assert.Equal("代理握手可用", viewModel.Items[0].Health);
        Assert.Equal(TimeSpan.FromMilliseconds(28), viewModel.Items[0].Latency);
        Assert.Equal("未验证", viewModel.Items[1].Health);
        Assert.Contains("不代表公网出口", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    private sealed class RecordingOutboundService : IOutboundService
    {
        private readonly List<OutboundListItem> _items = [];

        public OutboundImportDraft? LastDraft { get; private set; }

        public string? LastTestedOutboundId { get; private set; }

        public void Seed(params OutboundListItem[] items)
        {
            _items.AddRange(items);
        }

        public Task<IReadOnlyList<OutboundListItem>> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult<IReadOnlyList<OutboundListItem>>(
                _items.ToArray());
        }

        public Task<OutboundImportResult> ImportAsync(
            OutboundImportDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastDraft = draft;
            var item = new OutboundListItem(
                draft.Id,
                draft.Id,
                draft.Kind.ToString(),
                $"{draft.Host}:{draft.Port}",
                "未验证",
                null,
                null);
            _items.Add(item);
            return Task.FromResult(new OutboundImportResult(
                "import-1",
                [item],
                Array.Empty<string>()));
        }

        public Task<OutboundTestResult> TestAsync(
            string outboundId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastTestedOutboundId = outboundId;
            return Task.FromResult(new OutboundTestResult(
                outboundId,
                true,
                "代理握手可用",
                TimeSpan.FromMilliseconds(28),
                DateTimeOffset.Parse(
                    "2026-07-31T01:02:03Z",
                    CultureInfo.InvariantCulture),
                "代理握手成功；该结果不代表公网出口 IP 或最终规则路径已经验证。"));
        }
    }
}
