using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;

namespace NonProxy.Desktop.Tests;

public sealed class GatewaySystemStatusServiceTests
{
    [Fact]
    public async Task OverviewUsesAuthoritativeRetainedDecisionCount()
    {
        var client = new StubControlRpcClient
        {
            StatusResponse = new GetSystemStatusResponse
            {
                DataPlaneEnabled = true,
                ActiveSnapshotVersion = 4,
            },
            DecisionsResponse = new ListConnectionDecisionsResponse
            {
                TotalCount = 37,
                Page = new NonProxy.Common.V1.PageResponse(),
            },
        };
        var service = new GatewaySystemStatusService(
            client,
            new DisconnectedPolicyService(),
            new InstalledComponent());

        var overview = await service.GetOverviewAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal(37, overview.RecentDecisionCount);
        Assert.Equal(1, client.LastDecisionPageSize);
        Assert.Equal(ConnectionState.Connected, overview.Connection);
        Assert.True(overview.DataPlaneEnabled);
    }

    [Fact]
    public async Task OverviewCountsOnlyCurrentDirectNetworkRules()
    {
        var client = new StubControlRpcClient
        {
            StatusResponse = new GetSystemStatusResponse(),
            DecisionsResponse = new ListConnectionDecisionsResponse
            {
                Page = new NonProxy.Common.V1.PageResponse(),
            },
        };
        var policies = new FixedPolicyService(
            NetworkPolicy("home", PolicyApplyState.Active),
            NetworkPolicy("old", PolicyApplyState.PendingRemoval));
        var service = new GatewaySystemStatusService(
            client,
            policies,
            new InstalledComponent());

        var overview = await service.GetOverviewAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal(1, overview.DirectNetworkCount);
        Assert.Equal(1, overview.ActiveDirectRuleCount);
    }

    private static PolicyListItem NetworkPolicy(
        string id,
        PolicyApplyState state)
    {
        return new PolicyListItem(
            $"policy-{id}",
            $"{id} 直连",
            PolicyScope.Network,
            id,
            PolicyAction.Direct,
            state,
            1,
            DateTimeOffset.UtcNow,
            1,
            state == PolicyApplyState.Active ? 1UL : null);
    }

    private sealed class InstalledComponent : ISystemComponentInstaller
    {
        public Task<SystemComponentState> GetStateAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new SystemComponentState(
                SystemComponentStatus.Installed,
                "系统组件已安装"));
        }

        public Task<InstallResult> InstallAsync(CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }

        public Task<InstallResult> UninstallAsync(CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }

        public Task<InstallResult> OpenSystemSettingsAsync(
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }
    }

    private sealed class FixedPolicyService(
        params PolicyListItem[] policies) : IPolicyService
    {
        public Task<PolicyCatalog> GetCatalogAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new PolicyCatalog(
                policies,
                1,
                DateTimeOffset.UtcNow));
        }

        public Task<ApplyResult> SaveAsync(
            PolicyDraft draft,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }

        public Task<ApplyResult> DeleteAsync(
            string policyId,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }

        public Task<ApplyResult> RollBackAsync(
            ulong snapshotVersion,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }
    }
}
