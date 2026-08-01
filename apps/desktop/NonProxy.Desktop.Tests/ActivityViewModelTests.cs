using NonProxy.Desktop.Core.Features.Activity;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class ActivityViewModelTests
{
    [Fact]
    public async Task SignedHelperActivityCreatesConfirmedParentApplicationRule()
    {
        var activity = new FixedActivityService(Activity(
            8,
            "支付助手",
            "com.example.pay.helper",
            signerId: "TEAM-PAY",
            parentStableId: "com.example.pay"));
        var policies = new RecordingPolicyService();
        var viewModel = new ActivityViewModel(
            activity,
            policies,
            new TestPlatformInformation(PlatformKind.MacOS));
        await viewModel.RefreshCommand.ExecuteAsync(null);
        var item = Assert.Single(viewModel.Items);

        viewModel.RequestDirectCommand.Execute(item);

        var confirmation = Assert.IsType<ActivityDirectConfirmation>(
            viewModel.DirectConfirmation);
        Assert.Equal("com.example.pay", confirmation.RuleStableId);
        Assert.Contains("等待系统网络组件确认", confirmation.ActivationDetail);

        await viewModel.ConfirmDirectCommand.ExecuteAsync(null);

        var draft = Assert.IsType<PolicyDraft>(policies.LastSavedDraft);
        Assert.Equal("支付助手", draft.Name);
        Assert.Equal(PolicyScope.Application, draft.Scope);
        Assert.Equal("com.example.pay", draft.MatchValue);
        Assert.Equal(PolicyAction.Direct, draft.Action);
        Assert.Equal("TEAM-PAY", draft.ApplicationSignerId);
        Assert.True(draft.IncludeApplicationHelpers);
        Assert.Equal("规则已保存，等待系统组件确认。", viewModel.OperationMessage);
        Assert.Null(viewModel.DirectConfirmation);
        Assert.Equal("已有应用规则", Assert.Single(viewModel.Items).QuickActionStatus);
    }

    [Fact]
    public async Task UnsafeOrConflictingActivityNeverOffersQuickRuleCreation()
    {
        var activity = new FixedActivityService(
            Activity(1, "未签名", "com.example.unsigned"),
            Activity(
                2,
                "系统服务",
                "com.example.system",
                signerId: "APPLE",
                isSystemDecision: true),
            Activity(
                3,
                "Windows 应用",
                "example.exe",
                PlatformKind.Windows,
                "PUBLISHER"),
            Activity(
                4,
                "已配置",
                "com.example.configured",
                signerId: "TEAM-CONFIGURED"));
        var existing = new PolicyListItem(
            "existing",
            "已配置",
            PolicyScope.Application,
            "com.example.configured",
            PolicyAction.Proxy,
            PolicyApplyState.Active,
            4,
            DateTimeOffset.UtcNow);
        var policies = new RecordingPolicyService(existing);
        var viewModel = new ActivityViewModel(
            activity,
            policies,
            new TestPlatformInformation(PlatformKind.MacOS));

        await viewModel.RefreshCommand.ExecuteAsync(null);

        var byApplication = viewModel.Items.ToDictionary(item => item.Application);
        Assert.Equal("应用身份不足", byApplication["未签名"].QuickActionStatus);
        Assert.Equal("系统保护流量", byApplication["系统服务"].QuickActionStatus);
        Assert.Equal("其他平台记录", byApplication["Windows 应用"].QuickActionStatus);
        Assert.Equal("已有应用规则", byApplication["已配置"].QuickActionStatus);
        Assert.All(viewModel.Items, item => Assert.False(item.CanPrepareDirect));

        foreach (var item in viewModel.Items)
        {
            viewModel.RequestDirectCommand.Execute(item);
        }
        Assert.Null(viewModel.DirectConfirmation);
        Assert.Null(policies.LastSavedDraft);
    }

    internal static ActivityItem Activity(
        long sequence,
        string application,
        string stableId,
        PlatformKind platform = PlatformKind.MacOS,
        string? signerId = null,
        string? parentStableId = null,
        bool isSystemDecision = false)
    {
        return new ActivityItem(
            sequence,
            DateTimeOffset.UnixEpoch.AddSeconds(sequence),
            platform,
            stableId,
            signerId,
            parentStableId,
            parentStableId,
            isSystemDecision,
            application,
            "api.example.com · 443",
            "代理",
            "使用默认路由",
            "路径已确认",
            "代理出口 office",
            string.Empty,
            4);
    }

    internal sealed class FixedActivityService(
        params ActivityItem[] items) : IActivityService
    {
        public Task<IReadOnlyList<ActivityItem>> GetRecentAsync(
            int limit,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult<IReadOnlyList<ActivityItem>>(
                items.Take(limit).ToArray());
        }
    }

    internal sealed class RecordingPolicyService(
        params PolicyListItem[] initial) : IPolicyService
    {
        private readonly List<PolicyListItem> _items = [.. initial];

        public PolicyDraft? LastSavedDraft { get; private set; }

        public Task<PolicyCatalog> GetCatalogAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new PolicyCatalog(
                _items.ToArray(),
                4,
                DateTimeOffset.UtcNow));
        }

        public Task<ApplyResult> SaveAsync(
            PolicyDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastSavedDraft = draft;
            _items.Add(new PolicyListItem(
                "activity-created",
                draft.Name,
                draft.Scope,
                draft.MatchValue,
                draft.Action,
                PolicyApplyState.Pending,
                5,
                DateTimeOffset.UtcNow));
            return Task.FromResult(new ApplyResult(
                true,
                false,
                "NP_POLICY_PENDING",
                "规则已保存，等待系统组件确认。",
                5));
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
    }

    internal sealed record TestPlatformInformation(
        PlatformKind Platform) : IPlatformInformation
    {
        public string DisplayName => Platform.ToString();
    }
}
