using NonProxy.Desktop.Core.Features.Subscriptions;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class SubscriptionsViewModelTests
{
    [Fact]
    public async Task LoadClassifiesSubscriptionHealthWithoutExposingEndpoint()
    {
        var service = new RecordingSubscriptionService(
            CreateItem("healthy", "日常订阅"),
            CreateItem("attention", "需要处理", failures: 2),
            CreateItem("disabled", "已停用", enabled: false));
        var viewModel = new SubscriptionsViewModel(service);

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal(3, viewModel.Items.Count);
        Assert.Equal(2, viewModel.ActiveCount);
        Assert.Equal(1, viewModel.AttentionCount);
        Assert.Equal(1, viewModel.DisabledCount);
        Assert.Contains("2 个自动同步", viewModel.SyncSummary, StringComparison.Ordinal);
        var healthy = viewModel.Items.Single(item => item.Id == "healthy");
        var attention = viewModel.Items.Single(item => item.Id == "attention");
        var disabled = viewModel.Items.Single(item => item.Id == "disabled");
        Assert.True(healthy.IsHealthy);
        Assert.Equal("同步正常", healthy.StatusLabel);
        Assert.Contains("4 个节点", healthy.StatusDetail, StringComparison.Ordinal);
        Assert.True(attention.NeedsAttention);
        Assert.Contains("连续失败 2 次", attention.StatusDetail, StringComparison.Ordinal);
        Assert.Contains("连接超时", attention.StatusDetail, StringComparison.Ordinal);
        Assert.True(disabled.IsDisabled);
        Assert.Contains("不参与自动刷新", disabled.StatusDetail, StringComparison.Ordinal);
        Assert.DoesNotContain(
            viewModel.Items.SelectMany(ItemPresentation),
            value => value.Contains("https://", StringComparison.OrdinalIgnoreCase));
    }

    [Fact]
    public async Task EditingNeverLoadsStoredEndpointIntoTheForm()
    {
        var service = new RecordingSubscriptionService(CreateItem("daily", "日常订阅"));
        var viewModel = new SubscriptionsViewModel(service);
        await viewModel.RefreshCommand.ExecuteAsync(null);

        viewModel.EditCommand.Execute(viewModel.Items.Single());

        Assert.True(viewModel.IsEditorOpen);
        Assert.True(viewModel.IsEditing);
        Assert.Equal("日常订阅", viewModel.DisplayName);
        Assert.Empty(viewModel.EndpointUrl);
        Assert.Contains("留空", viewModel.EndpointHint, StringComparison.Ordinal);
        Assert.Contains("系统凭据库", viewModel.EndpointHint, StringComparison.Ordinal);
    }

    [Fact]
    public async Task CreateGeneratesInternalIdAndClearsSensitiveInput()
    {
        var service = new RecordingSubscriptionService();
        var viewModel = new SubscriptionsViewModel(service);
        viewModel.OpenCreateCommand.Execute(null);
        viewModel.DisplayName = "备用订阅";
        viewModel.EndpointUrl = "https://provider.example/sub?token=private";

        await viewModel.SaveCommand.ExecuteAsync(null);

        var draft = Assert.IsType<SubscriptionDraft>(service.LastSavedDraft);
        Assert.StartsWith("subscription-", draft.Id, StringComparison.Ordinal);
        Assert.Equal("备用订阅", draft.DisplayName);
        Assert.Equal("https://provider.example/sub?token=private", draft.EndpointUrl);
        Assert.Empty(viewModel.EndpointUrl);
        Assert.False(viewModel.IsEditorOpen);
        Assert.Single(viewModel.Items);
    }

    [Fact]
    public async Task InvalidEndpointIsRejectedBeforeLeavingTheEditor()
    {
        var service = new RecordingSubscriptionService();
        var viewModel = new SubscriptionsViewModel(service);
        viewModel.OpenCreateCommand.Execute(null);
        viewModel.DisplayName = "不安全订阅";
        viewModel.EndpointUrl = "http://provider.example/subscription";

        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.Null(service.LastSavedDraft);
        Assert.True(viewModel.IsEditorOpen);
        Assert.Contains("HTTPS", viewModel.ValidationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task TogglePreservesCredentialAndChangesOnlyEnabledSetting()
    {
        var service = new RecordingSubscriptionService(CreateItem("daily", "日常订阅"));
        var viewModel = new SubscriptionsViewModel(service);
        await viewModel.RefreshCommand.ExecuteAsync(null);
        var item = viewModel.Items.Single();

        await viewModel.ToggleEnabledCommand.ExecuteAsync(item);

        var draft = Assert.IsType<SubscriptionDraft>(service.LastSavedDraft);
        Assert.Equal("daily", draft.Id);
        Assert.Null(draft.EndpointUrl);
        Assert.False(draft.Enabled);
        Assert.True(viewModel.Items.Single().IsDisabled);
        Assert.Contains("已停用", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task DeleteRequiresExplicitConfirmation()
    {
        var service = new RecordingSubscriptionService(CreateItem("daily", "日常订阅"));
        var viewModel = new SubscriptionsViewModel(service);
        await viewModel.RefreshCommand.ExecuteAsync(null);
        var item = viewModel.Items.Single();

        viewModel.RequestDeleteCommand.Execute(item);

        Assert.True(item.IsDeletePending);
        Assert.Equal(0, service.DeleteCallCount);

        await viewModel.ConfirmDeleteCommand.ExecuteAsync(item);

        Assert.Equal(1, service.DeleteCallCount);
        Assert.Empty(viewModel.Items);
        Assert.Contains("已删除", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task RejectedMutationKeepsSubscriptionAndShowsSafeMessage()
    {
        var service = new RecordingSubscriptionService(CreateItem("daily", "日常订阅"))
        {
            RejectRefresh = true,
        };
        var viewModel = new SubscriptionsViewModel(service);
        await viewModel.RefreshCommand.ExecuteAsync(null);
        viewModel.OperationMessage = "上一次操作成功。";

        await viewModel.RefreshSourceCommand.ExecuteAsync(viewModel.Items.Single());

        Assert.Single(viewModel.Items);
        Assert.Equal("订阅暂时无法刷新。", viewModel.ErrorMessage);
        Assert.Null(viewModel.OperationMessage);
    }

    private static IEnumerable<string> ItemPresentation(SubscriptionViewItem item)
    {
        yield return item.DisplayName;
        yield return item.StatusLabel;
        yield return item.StatusDetail;
        yield return item.ScheduleLabel;
        yield return item.LastSuccessLabel;
        yield return item.IntervalLabel;
        yield return item.GenerationLabel;
    }

    private static SubscriptionListItem CreateItem(
        string id,
        string displayName,
        bool enabled = true,
        uint failures = 0)
    {
        return new SubscriptionListItem(
            id,
            displayName,
            enabled,
            TimeSpan.FromHours(6),
            7,
            3,
            failures,
            DateTimeOffset.UtcNow.AddHours(6),
            DateTimeOffset.UtcNow,
            failures == 0 ? DateTimeOffset.UtcNow : null,
            failures == 0 ? null : "NP_SUBSCRIPTION_TIMEOUT",
            4);
    }

    private sealed class RecordingSubscriptionService(
        params SubscriptionListItem[] items) : ISubscriptionService
    {
        private readonly List<SubscriptionListItem> _items = [.. items];

        public SubscriptionDraft? LastSavedDraft { get; private set; }

        public int DeleteCallCount { get; private set; }

        public bool RejectRefresh { get; init; }

        public Task<SubscriptionCatalog> ListAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new SubscriptionCatalog(
                _items.ToArray(),
                DateTimeOffset.UtcNow));
        }

        public Task<SubscriptionMutation> SaveAsync(
            SubscriptionDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastSavedDraft = draft;
            var existing = _items.SingleOrDefault(item => item.Id == draft.Id);
            var saved = new SubscriptionListItem(
                draft.Id,
                draft.DisplayName,
                draft.Enabled,
                draft.RefreshInterval,
                (existing?.Revision ?? 0) + 1,
                existing?.ContentGeneration ?? 1,
                0,
                DateTimeOffset.UtcNow.Add(draft.RefreshInterval),
                DateTimeOffset.UtcNow,
                DateTimeOffset.UtcNow,
                null,
                existing?.NodeCount ?? 2);
            _items.RemoveAll(item => item.Id == draft.Id);
            _items.Add(saved);
            return Task.FromResult(AcceptedMutation(saved, "订阅已保存。"));
        }

        public Task<SubscriptionMutation> RefreshAsync(
            string sourceId,
            ulong expectedRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var item = _items.Single(entry => entry.Id == sourceId);
            if (RejectRefresh)
            {
                return Task.FromResult(new SubscriptionMutation(
                    false,
                    "NP_SUBSCRIPTION_CONNECT_FAILED",
                    "订阅暂时无法刷新。",
                    item,
                    false,
                    []));
            }
            return Task.FromResult(AcceptedMutation(item, "订阅已刷新。"));
        }

        public Task<SubscriptionDeletion> DeleteAsync(
            string sourceId,
            ulong expectedRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            DeleteCallCount++;
            _items.RemoveAll(item => item.Id == sourceId);
            return Task.FromResult(new SubscriptionDeletion(
                true,
                "NP_SUBSCRIPTION_DELETED",
                "订阅已删除。",
                sourceId,
                2,
                []));
        }

        private static SubscriptionMutation AcceptedMutation(
            SubscriptionListItem item,
            string message)
        {
            return new SubscriptionMutation(
                true,
                "NP_SUBSCRIPTION_SAVED",
                message,
                item,
                false,
                []);
        }
    }
}
