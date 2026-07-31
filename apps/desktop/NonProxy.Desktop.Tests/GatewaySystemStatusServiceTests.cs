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
}
