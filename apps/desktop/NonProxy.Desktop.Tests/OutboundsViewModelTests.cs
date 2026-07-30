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

    private sealed class RecordingOutboundService : IOutboundService
    {
        private readonly List<OutboundListItem> _items = [];

        public OutboundImportDraft? LastDraft { get; private set; }

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
                null);
            _items.Add(item);
            return Task.FromResult(new OutboundImportResult(
                "import-1",
                [item],
                Array.Empty<string>()));
        }
    }
}
