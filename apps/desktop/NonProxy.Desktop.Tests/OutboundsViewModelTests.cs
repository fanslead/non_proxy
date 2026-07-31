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
                null,
                SupportsDefaultRoute: true));
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

    [Fact]
    public async Task ExitVerificationRefreshesOnlyTheSelectedRoute()
    {
        var outboundService = new RecordingOutboundService();
        outboundService.Seed(
            new OutboundListItem(
                "office",
                "Office",
                "SOCKS5",
                "127.0.0.1:1080",
                "未验证",
                null,
                null,
                CanVerifyExit: true));
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        await viewModel.VerifyExitCommand.ExecuteAsync(viewModel.Items[0]);

        Assert.Equal("office", outboundService.LastVerifiedOutboundId);
        Assert.False(outboundService.LastExitRouteWasDirect);
        Assert.Equal("8.8.8.8", Assert.Single(viewModel.Items).ExitReceipt?.ObservedIp);
        Assert.Null(viewModel.DirectExitReceipt);

        await viewModel.VerifyDirectExitCommand.ExecuteAsync(null);

        Assert.True(outboundService.LastExitRouteWasDirect);
        Assert.Null(outboundService.LastVerifiedOutboundId);
        Assert.Equal("1.1.1.1", viewModel.DirectExitReceipt?.ObservedIp);
        Assert.Equal("8.8.8.8", Assert.Single(viewModel.Items).ExitReceipt?.ObservedIp);
        Assert.Contains("签名验证", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task ExitVerificationCommandsStayDisabledWithoutTrustedProbe()
    {
        var outboundService = new RecordingOutboundService
        {
            ExitVerificationAvailable = false,
        };
        outboundService.Seed(
            new OutboundListItem(
                "office",
                "Office",
                "SOCKS5",
                "127.0.0.1:1080",
                "未验证",
                null,
                null,
                CanVerifyExit: true));
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.False(viewModel.VerifyDirectExitCommand.CanExecute(null));
        Assert.False(viewModel.VerifyExitCommand.CanExecute(viewModel.Items[0]));
        Assert.Contains(
            "尚未配置",
            viewModel.ExitVerificationAvailabilityMessage,
            StringComparison.Ordinal);
    }

    [Fact]
    public async Task SetDefaultReloadsOnlyAfterServerAcceptsPendingSnapshot()
    {
        var outboundService = new RecordingOutboundService();
        outboundService.Seed(
            new OutboundListItem(
                "office",
                "Office",
                "SOCKS5",
                "127.0.0.1:1080",
                "未验证",
                null,
                null,
                SupportsDefaultRoute: true));
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        await viewModel.SetDefaultCommand.ExecuteAsync(viewModel.Items[0]);

        Assert.Equal("office", outboundService.LastDefaultOutboundId);
        Assert.Equal<ulong>(1, outboundService.LastExpectedRoutingRevision);
        Assert.True(Assert.Single(viewModel.Items).IsDefault);
        Assert.Contains("等待系统组件确认", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task RestoreDirectClearsDefaultProxyAfterServerAcceptance()
    {
        var outboundService = new RecordingOutboundService();
        outboundService.Seed(
            new OutboundListItem(
                "office",
                "Office",
                "SOCKS5",
                "127.0.0.1:1080",
                "未验证",
                null,
                null,
                IsDefault: true,
                SupportsDefaultRoute: true));
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        await viewModel.SetDirectCommand.ExecuteAsync(null);

        Assert.True(outboundService.LastRouteWasDirect);
        Assert.False(Assert.Single(viewModel.Items).IsDefault);
        Assert.Contains("默认直连", viewModel.DefaultRouteSummary, StringComparison.Ordinal);
        Assert.Contains("等待系统组件确认", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task DisconnectedCatalogDoesNotClaimDirectConfiguration()
    {
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(
                    new DisconnectedOutboundService()));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Contains("无法读取", viewModel.DefaultRouteSummary, StringComparison.Ordinal);
        Assert.False(viewModel.SetDirectCommand.CanExecute(null));
        Assert.False(viewModel.VerifyDirectExitCommand.CanExecute(null));
    }

    [Fact]
    public async Task UriImportRequiresPreviewAndInvalidatesItWhenTheSourceChanges()
    {
        var outboundService = new RecordingOutboundService();
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        const string source =
            "socks5://alice:private@proxy.example:1080#office-proxy";

        viewModel.UriImportText = source;
        Assert.False(viewModel.SaveUriImportCommand.CanExecute(null));
        await viewModel.PreviewUriImportCommand.ExecuteAsync(null);

        Assert.Equal(source, outboundService.LastPreviewedUriList);
        Assert.Single(viewModel.UriImportPreview);
        Assert.True(viewModel.HasUriImportPreview);
        Assert.True(viewModel.SaveUriImportCommand.CanExecute(null));
        Assert.Null(outboundService.LastImportedUriList);

        viewModel.UriImportText += "\nhttp://proxy.example:8080#backup";

        Assert.Empty(viewModel.UriImportPreview);
        Assert.False(viewModel.HasUriImportPreview);
        Assert.False(viewModel.SaveUriImportCommand.CanExecute(null));
    }

    [Fact]
    public async Task SavingAReviewedUriListClearsTheSecretBearingInput()
    {
        var outboundService = new RecordingOutboundService();
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        const string source =
            "socks5://alice:private@proxy.example:1080#office-proxy";
        viewModel.UriImportText = source;
        await viewModel.PreviewUriImportCommand.ExecuteAsync(null);

        await viewModel.SaveUriImportCommand.ExecuteAsync(null);

        Assert.Equal(source, outboundService.LastImportedUriList);
        Assert.Equal(string.Empty, viewModel.UriImportText);
        Assert.Empty(viewModel.UriImportPreview);
        Assert.Contains("系统凭据库", viewModel.UriImportMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task LatePreviewCannotAuthorizeAChangedSource()
    {
        var completion = new TaskCompletionSource<OutboundImportResult>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var outboundService = new RecordingOutboundService
        {
            UriPreviewCompletion = completion,
        };
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        viewModel.UriImportText = "socks5://proxy.example:1080#office";

        var preview = viewModel.PreviewUriImportCommand.ExecuteAsync(null);
        viewModel.UriImportText = "http://proxy.example:8080#changed";
        completion.SetResult(RecordingOutboundService.UriImportResult());
        await preview;

        Assert.Empty(viewModel.UriImportPreview);
        Assert.False(viewModel.SaveUriImportCommand.CanExecute(null));
    }

    [Fact]
    public async Task SaveDoesNotEraseInputChangedWhileTheReviewedSourceIsSaving()
    {
        var outboundService = new RecordingOutboundService();
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IOutboundService>(outboundService));
        var viewModel = services.GetRequiredService<OutboundsViewModel>();
        const string reviewed = "socks5://proxy.example:1080#office";
        const string changed = "http://proxy.example:8080#changed";
        viewModel.UriImportText = reviewed;
        await viewModel.PreviewUriImportCommand.ExecuteAsync(null);
        var completion = new TaskCompletionSource<OutboundImportResult>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        outboundService.UriImportCompletion = completion;

        var save = viewModel.SaveUriImportCommand.ExecuteAsync(null);
        viewModel.UriImportText = changed;
        completion.SetResult(RecordingOutboundService.UriImportResult());
        await save;

        Assert.Equal(reviewed, outboundService.LastImportedUriList);
        Assert.Equal(changed, viewModel.UriImportText);
        Assert.Contains("没有自动清空", viewModel.UriImportMessage, StringComparison.Ordinal);
    }

    private sealed class RecordingOutboundService : IOutboundService
    {
        private readonly List<OutboundListItem> _items = [];

        public OutboundImportDraft? LastDraft { get; private set; }

        public string? LastPreviewedUriList { get; private set; }

        public string? LastImportedUriList { get; private set; }

        public TaskCompletionSource<OutboundImportResult>? UriPreviewCompletion { get; init; }

        public TaskCompletionSource<OutboundImportResult>? UriImportCompletion { get; set; }

        public string? LastTestedOutboundId { get; private set; }

        public string? LastVerifiedOutboundId { get; private set; }

        public bool LastExitRouteWasDirect { get; private set; }

        public string? LastDefaultOutboundId { get; private set; }

        public ulong LastExpectedRoutingRevision { get; private set; }

        public bool LastRouteWasDirect { get; private set; }

        public bool ExitVerificationAvailable { get; init; } = true;

        private ulong RoutingRevision { get; set; } = 1;

        public void Seed(params OutboundListItem[] items)
        {
            _items.AddRange(items);
        }

        public Task<OutboundCatalog> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundCatalog(
                _items.ToArray(),
                RoutingRevision,
                _items.SingleOrDefault(item => item.IsDefault)?.Id,
                ExitVerificationAvailable,
                DirectExitReceipt: DirectReceipt));
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

        public Task<OutboundImportResult> PreviewUriListAsync(
            string uriList,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastPreviewedUriList = uriList;
            return UriPreviewCompletion?.Task ?? Task.FromResult(UriImportResult());
        }

        public async Task<OutboundImportResult> ImportUriListAsync(
            string uriList,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastImportedUriList = uriList;
            var result = UriImportCompletion is null
                ? UriImportResult()
                : await UriImportCompletion.Task.WaitAsync(cancellationToken);
            _items.AddRange(result.Outbounds);
            return result;
        }

        public static OutboundImportResult UriImportResult()
        {
            return new OutboundImportResult(
                "uri-import-1",
                [
                    new OutboundListItem(
                        "office-proxy",
                        "office-proxy",
                        "SOCKS5",
                        "proxy.example:1080",
                        "未验证",
                        null,
                        null),
                ],
                []);
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

        public Task<ExitVerificationResult> VerifyExitAsync(
            string? outboundId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastVerifiedOutboundId = outboundId;
            LastExitRouteWasDirect = string.IsNullOrWhiteSpace(outboundId);
            var receipt = new ExitVerificationReceipt(
                1,
                "A".PadRight(43, 'A'),
                outboundId is null ? "1.1.1.1" : "8.8.8.8",
                DateTimeOffset.Parse(
                    "2026-07-31T01:02:03Z",
                    CultureInfo.InvariantCulture),
                DateTimeOffset.Parse(
                    "2026-07-31T01:02:04Z",
                    CultureInfo.InvariantCulture),
                outboundId);
            if (outboundId is null)
            {
                DirectReceipt = receipt;
            }
            else
            {
                for (var index = 0; index < _items.Count; index++)
                {
                    if (string.Equals(
                            _items[index].Id,
                            outboundId,
                            StringComparison.Ordinal))
                    {
                        _items[index] = _items[index] with
                        {
                            ExitReceipt = receipt,
                        };
                    }
                }
            }
            return Task.FromResult(new ExitVerificationResult(
                true,
                "NP_EXIT_PROBE_VERIFIED",
                $"公网出口已签名验证：{receipt.ObservedIp}"));
        }

        private ExitVerificationReceipt? DirectReceipt { get; set; }

        public Task<ApplyResult> SetDefaultAsync(
            string outboundId,
            ulong expectedRoutingRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastDefaultOutboundId = outboundId;
            LastExpectedRoutingRevision = expectedRoutingRevision;
            if (expectedRoutingRevision != RoutingRevision)
            {
                return Task.FromResult(new ApplyResult(
                    false,
                    false,
                    "NP_ROUTING_REVISION_CONFLICT",
                    "默认路由已变化。",
                    null));
            }

            RoutingRevision++;
            for (var index = 0; index < _items.Count; index++)
            {
                _items[index] = _items[index] with
                {
                    IsDefault = string.Equals(
                        _items[index].Id,
                        outboundId,
                        StringComparison.Ordinal),
                };
            }

            return Task.FromResult(new ApplyResult(
                true,
                false,
                "NP_SNAPSHOT_PENDING_ACK",
                "默认代理已保存，新的路由快照正在等待系统组件确认。",
                1));
        }

        public Task<ApplyResult> SetDirectAsync(
            ulong expectedRoutingRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastExpectedRoutingRevision = expectedRoutingRevision;
            LastRouteWasDirect = true;
            if (expectedRoutingRevision != RoutingRevision)
            {
                return Task.FromResult(new ApplyResult(
                    false,
                    false,
                    "NP_ROUTING_REVISION_CONFLICT",
                    "默认路由已变化。",
                    null));
            }

            RoutingRevision++;
            for (var index = 0; index < _items.Count; index++)
            {
                _items[index] = _items[index] with { IsDefault = false };
            }

            return Task.FromResult(new ApplyResult(
                true,
                false,
                "NP_SNAPSHOT_PENDING_ACK",
                "默认直连已保存，新的路由快照正在等待系统组件确认。",
                2));
        }
    }
}
