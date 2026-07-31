using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Applications;
using NonProxy.Desktop.Core.Features.Websites;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class PolicyViewModelTests
{
    [Fact]
    public async Task WebsiteRuleNormalizesDomainBeforeSaving()
    {
        var policyService = new RecordingPolicyService();
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IPolicyService>(policyService));
        var viewModel = services.GetRequiredService<WebsitesViewModel>();
        viewModel.Domain = " Example.COM. ";

        await viewModel.AddCommand.ExecuteAsync(null);

        Assert.NotNull(policyService.LastSavedDraft);
        Assert.Equal("example.com", policyService.LastSavedDraft.MatchValue);
        Assert.Equal(PolicyScope.Website, policyService.LastSavedDraft.Scope);
        Assert.Equal(PolicyAction.Direct, policyService.LastSavedDraft.Action);
        Assert.Empty(viewModel.Domain);
        Assert.Single(viewModel.Items);
    }

    [Fact]
    public async Task InvalidWebsiteRuleNeverReachesControlService()
    {
        var policyService = new RecordingPolicyService();
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IPolicyService>(policyService));
        var viewModel = services.GetRequiredService<WebsitesViewModel>();
        viewModel.Domain = "https://example.com/account";

        await viewModel.AddCommand.ExecuteAsync(null);

        Assert.Null(policyService.LastSavedDraft);
        Assert.Contains("不要包含协议", viewModel.ValidationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task ApplicationRuleUsesStableIdentityInsteadOfDestinationGuessing()
    {
        var policyService = new RecordingPolicyService();
        var applicationCatalog = new TestApplicationCatalog(
            new ApplicationCatalogEntry(
                "办公软件",
                "com.example.office",
                "TEAM123",
                "com.example.office",
                true));
        using var services = TestPlatformServices.Create(
            configure: collection =>
            {
                collection.AddSingleton<IPolicyService>(policyService);
                collection.AddSingleton<IApplicationCatalog>(applicationCatalog);
            });
        var viewModel = services.GetRequiredService<ApplicationsViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);
        var application = Assert.Single(viewModel.AvailableApplications);

        await viewModel.AddCommand.ExecuteAsync(application);

        Assert.NotNull(policyService.LastSavedDraft);
        Assert.Equal(PolicyScope.Application, policyService.LastSavedDraft.Scope);
        Assert.Equal("com.example.office", policyService.LastSavedDraft.MatchValue);
        Assert.Equal("TEAM123", policyService.LastSavedDraft.ApplicationSignerId);
        Assert.True(policyService.LastSavedDraft.IncludeApplicationHelpers);
        Assert.Single(viewModel.Items);
    }

    [Fact]
    public async Task ApplicationSearchMatchesFriendlyNameWithoutShowingIdentityInput()
    {
        var applicationCatalog = new TestApplicationCatalog(
            new ApplicationCatalogEntry(
                "企业办公",
                "com.example.office",
                "TEAM123",
                "com.example.office",
                false),
            new ApplicationCatalogEntry(
                "聊天工具",
                "com.example.chat",
                null,
                "com.example.chat",
                true));
        using var services = TestPlatformServices.Create(
            configure: collection =>
                collection.AddSingleton<IApplicationCatalog>(applicationCatalog));
        var viewModel = services.GetRequiredService<ApplicationsViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        viewModel.SearchText = "办公";

        var result = Assert.Single(viewModel.AvailableApplications);
        Assert.Equal("企业办公", result.DisplayName);
        Assert.False(result.IsConfigured);
    }

    [Fact]
    public async Task ChosenApplicationIsSavedWithoutManualIdentityInput()
    {
        var policyService = new RecordingPolicyService();
        var applicationCatalog = new TestApplicationCatalog(
            new ApplicationCatalogEntry(
                "财务工具",
                "com.example.finance",
                "TEAM456",
                "com.example.finance",
                false));
        using var services = TestPlatformServices.Create(
            configure: collection =>
            {
                collection.AddSingleton<IPolicyService>(policyService);
                collection.AddSingleton<IApplicationCatalog>(applicationCatalog);
            });
        var viewModel = services.GetRequiredService<ApplicationsViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        await viewModel.ChooseCommand.ExecuteAsync(null);

        Assert.Equal(
            "com.example.finance",
            policyService.LastSavedDraft?.MatchValue);
        Assert.Equal("TEAM456", policyService.LastSavedDraft?.ApplicationSignerId);
    }

    private sealed class RecordingPolicyService : IPolicyService
    {
        private readonly List<PolicyListItem> _items = [];

        public PolicyDraft? LastSavedDraft { get; private set; }

        public Task<PolicyCatalog> GetCatalogAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new PolicyCatalog(
                _items.ToArray(),
                _items.Count == 0 ? null : 7,
                DateTimeOffset.UtcNow));
        }

        public Task<ApplyResult> SaveAsync(
            PolicyDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastSavedDraft = draft;
            _items.Add(new PolicyListItem(
                $"policy-{_items.Count + 1}",
                draft.Name,
                draft.Scope,
                draft.MatchValue,
                draft.Action,
                PolicyApplyState.Active,
                7,
                DateTimeOffset.UtcNow));
            return Task.FromResult(new ApplyResult(
                true,
                true,
                "NP_POLICY_APPLIED",
                "规则已应用。",
                7));
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

    private sealed class TestApplicationCatalog(
        params ApplicationCatalogEntry[] applications) : IApplicationCatalog
    {
        public Task<ApplicationCatalogSnapshot> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new ApplicationCatalogSnapshot(
                applications,
                true,
                true,
                "测试应用目录"));
        }

        public Task<ApplicationSelectionResult> ChooseAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new ApplicationSelectionResult(
                true,
                applications.FirstOrDefault(),
                "已选择测试应用"));
        }
    }
}
