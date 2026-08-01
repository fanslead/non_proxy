using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Policies;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class PoliciesRestoreViewModelTests
{
    [Fact]
    public async Task RestoreRequiresConfirmationAndRemainsPendingUntilProviderAck()
    {
        var policies = new RestorePolicyService();
        using var services = TestPlatformServices.Create(configure: registrations =>
            registrations.AddSingleton<IPolicyService>(policies));
        var viewModel = services.GetRequiredService<PoliciesViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        viewModel.RequestRestorePreviousCommand.Execute(null);

        Assert.True(viewModel.IsRestoreConfirmationVisible);
        Assert.Null(policies.RestoredSnapshotVersion);

        await viewModel.ConfirmRestorePreviousCommand.ExecuteAsync(null);

        Assert.Equal(7UL, policies.RestoredSnapshotVersion);
        Assert.False(viewModel.IsRestoreConfirmationVisible);
        Assert.False(viewModel.CanRestorePrevious);
        Assert.Contains("等待系统组件确认", viewModel.OperationMessage, StringComparison.Ordinal);
        Assert.Contains("等待系统组件确认", viewModel.RestorePreviousDetail, StringComparison.Ordinal);
    }

    [Fact]
    public async Task ExistingPendingSnapshotDisablesRestore()
    {
        var policies = new RestorePolicyService { HasPendingSnapshot = true };
        using var services = TestPlatformServices.Create(configure: registrations =>
            registrations.AddSingleton<IPolicyService>(policies));
        var viewModel = services.GetRequiredService<PoliciesViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        viewModel.RequestRestorePreviousCommand.Execute(null);

        Assert.False(viewModel.CanRestorePrevious);
        Assert.False(viewModel.IsRestoreConfirmationVisible);
        Assert.Null(policies.RestoredSnapshotVersion);
    }

    [Fact]
    public async Task RefreshClosesConfirmationWhenTheRestorePointChanges()
    {
        var policies = new RestorePolicyService();
        using var services = TestPlatformServices.Create(configure: registrations =>
            registrations.AddSingleton<IPolicyService>(policies));
        var viewModel = services.GetRequiredService<PoliciesViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);
        viewModel.RequestRestorePreviousCommand.Execute(null);
        Assert.True(viewModel.IsRestoreConfirmationVisible);

        policies.ActiveSnapshotVersion = 9;
        policies.PreviousSnapshotVersion = 8;
        await viewModel.RefreshCommand.ExecuteAsync(null);
        await viewModel.ConfirmRestorePreviousCommand.ExecuteAsync(null);

        Assert.False(viewModel.IsRestoreConfirmationVisible);
        Assert.Null(policies.RestoredSnapshotVersion);
    }

    internal sealed class RestorePolicyService : IPolicyService
    {
        public bool HasPendingSnapshot { get; set; }

        public ulong ActiveSnapshotVersion { get; set; } = 8;

        public ulong PreviousSnapshotVersion { get; set; } = 7;

        public ulong? RestoredSnapshotVersion { get; private set; }

        public Task<PolicyCatalog> GetCatalogAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new PolicyCatalog(
                [],
                ActiveSnapshotVersion,
                DateTimeOffset.UtcNow,
                HasPendingSnapshot ? 9UL : null,
                PreviousSnapshotVersion));
        }

        public Task<ApplyResult> RollBackAsync(
            ulong snapshotVersion,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            RestoredSnapshotVersion = snapshotVersion;
            HasPendingSnapshot = true;
            return Task.FromResult(new ApplyResult(
                true,
                false,
                "NP_POLICY_PENDING",
                "回滚请求已保存，快照 v9 正等待系统组件确认。",
                9));
        }

        public Task<ApplyResult> SaveAsync(
            PolicyDraft draft,
            CancellationToken cancellationToken) => throw Unused();

        public Task<ApplyResult> DeleteAsync(
            string policyId,
            CancellationToken cancellationToken) => throw Unused();

        private static NotSupportedException Unused()
        {
            return new NotSupportedException("本测试不调用规则编辑方法。");
        }
    }
}
