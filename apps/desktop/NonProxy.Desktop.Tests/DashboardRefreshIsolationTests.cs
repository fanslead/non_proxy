using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Services.Adapters;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class DashboardRefreshIsolationTests
{
    [Fact]
    public async Task RuntimeRefreshDoesNotReloadSetupCatalogsForEveryDecision()
    {
        var status = new CountingStatusService();
        var outbounds = new CountingOutboundService();
        var adapters = new CountingAdapterService();
        using var services = TestPlatformServices.Create(configure: registrations =>
        {
            registrations.AddSingleton<ISystemStatusService>(status);
            registrations.AddSingleton<IOutboundService>(outbounds);
            registrations.AddSingleton<IAdapterManagementService>(adapters);
        });
        var viewModel = services.GetRequiredService<DashboardViewModel>();

        await viewModel.RefreshCommand.ExecuteAsync(null);
        await viewModel.RefreshRuntimeCommand.ExecuteAsync(null);

        Assert.Equal(2, status.ReadCount);
        Assert.Equal(1, outbounds.ReadCount);
        Assert.Equal(1, adapters.ReadCount);
    }

    [Fact]
    public async Task InvalidOptionalCatalogContractIsNotHiddenAsUnavailable()
    {
        var outbounds = new CountingOutboundService
        {
            ReadFailure = new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "测试出口目录契约无效。"),
        };
        using var services = TestPlatformServices.Create(configure: registrations =>
            registrations.AddSingleton<IOutboundService>(outbounds));
        var viewModel = services.GetRequiredService<DashboardViewModel>();

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.True(viewModel.HasError);
        Assert.Equal("测试出口目录契约无效。", viewModel.ErrorMessage);
    }

    private sealed class CountingStatusService : ISystemStatusService
    {
        public int ReadCount { get; private set; }

        public Task<SystemOverview> GetOverviewAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ReadCount++;
            return Task.FromResult(SystemOverview.Unavailable(
                new NonProxy.Desktop.Core.Platform.SystemComponentState(
                    NonProxy.Desktop.Core.Platform.SystemComponentStatus.NotInstalled,
                    "测试组件未安装")));
        }
    }

    internal sealed class CountingOutboundService : IOutboundService
    {
        public int ReadCount { get; private set; }

        public ControlServiceException? ReadFailure { get; init; }

        public Task<OutboundCatalog> ListAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ReadCount++;
            if (ReadFailure is not null)
            {
                throw ReadFailure;
            }
            return Task.FromResult(new OutboundCatalog([], 1));
        }

        public Task<OutboundTestResult> TestAsync(
            string outboundId,
            CancellationToken cancellationToken) => throw Unused();

        public Task<ExitVerificationResult> VerifyExitAsync(
            string? outboundId,
            CancellationToken cancellationToken) => throw Unused();

        public Task<ApplyResult> SetDefaultAsync(
            string outboundId,
            ulong expectedRoutingRevision,
            CancellationToken cancellationToken) => throw Unused();

        public Task<ApplyResult> SetDirectAsync(
            ulong expectedRoutingRevision,
            CancellationToken cancellationToken) => throw Unused();

        public Task<OutboundImportResult> ImportAsync(
            OutboundImportDraft draft,
            CancellationToken cancellationToken) => throw Unused();

        public Task<OutboundImportResult> PreviewUriListAsync(
            string uriList,
            CancellationToken cancellationToken) => throw Unused();

        public Task<OutboundImportResult> ImportUriListAsync(
            string uriList,
            CancellationToken cancellationToken) => throw Unused();
    }

    internal sealed class CountingAdapterService : IAdapterManagementService
    {
        public int ReadCount { get; private set; }

        public Task<AdapterCatalog> ListAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ReadCount++;
            return Task.FromResult(new AdapterCatalog([], DateTimeOffset.UtcNow));
        }

        public Task<AdapterMutationResult> RegisterAsync(
            AdapterRegistrationDraft draft,
            CancellationToken cancellationToken) => throw Unused();

        public Task<AdapterMutationResult> RemoveAsync(
            string adapterId,
            CancellationToken cancellationToken) => throw Unused();

        public Task<AdapterSyncResult> SyncAsync(
            string adapterId,
            CancellationToken cancellationToken) => throw Unused();
    }

    private static NotSupportedException Unused()
    {
        return new NotSupportedException("本测试不调用变更方法。");
    }
}
