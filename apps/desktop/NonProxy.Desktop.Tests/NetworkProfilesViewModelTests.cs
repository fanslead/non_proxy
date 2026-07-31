using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Networks;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class NetworkProfilesViewModelTests
{
    private const string Fingerprint =
        "95e986531d4972a782f3a2a868cbecb194a0e0fc14f95280706077e9fbf63fc5";

    [Fact]
    public async Task OneClickCreatesProfileAndPendingDirectPolicy()
    {
        var profiles = new RecordingNetworkProfileService();
        var policies = new RecordingPolicyService(new ApplyResult(
            true,
            false,
            "NP_POLICY_PENDING",
            "规则已保存，快照 v9 正等待系统组件确认。",
            9));
        using var services = Services(profiles, policies);
        var viewModel = services.GetRequiredService<NetworkProfilesViewModel>();

        await viewModel.DetectCommand.ExecuteAsync(null);
        viewModel.NetworkName = "家庭网络";
        await viewModel.SaveDirectCommand.ExecuteAsync(null);

        Assert.Equal("家庭网络", profiles.LastSavedDraft?.DisplayName);
        Assert.Equal(PolicyScope.Network, policies.LastSavedDraft?.Scope);
        Assert.Equal(PolicyAction.Direct, policies.LastSavedDraft?.Action);
        Assert.Equal(profiles.Items.Single().Id, policies.LastSavedDraft?.MatchValue);
        Assert.Contains("等待系统组件确认", viewModel.OperationMessage, StringComparison.Ordinal);
        Assert.Contains("等待系统组件确认", Assert.Single(viewModel.Items).RuleStateLabel,
            StringComparison.Ordinal);
        Assert.Equal(0, profiles.DeleteCallCount);
    }

    [Fact]
    public async Task RejectedPolicyRollsBackNewProfile()
    {
        var profiles = new RecordingNetworkProfileService();
        var policies = new RecordingPolicyService(new ApplyResult(
            false,
            false,
            "NP_POLICY_COMPILE_REJECTED",
            "规则存在冲突。",
            null));
        using var services = Services(profiles, policies);
        var viewModel = services.GetRequiredService<NetworkProfilesViewModel>();

        await viewModel.DetectCommand.ExecuteAsync(null);
        await viewModel.SaveDirectCommand.ExecuteAsync(null);

        Assert.Equal(1, profiles.DeleteCallCount);
        Assert.Empty(profiles.Items);
        Assert.Empty(viewModel.Items);
        Assert.Contains("已回收", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task ExistingActiveRuleIsNotMutatedAgain()
    {
        var profile = NetworkProfile("office", "办公室");
        var profiles = new RecordingNetworkProfileService(profile);
        var policies = new RecordingPolicyService(
            new ApplyResult(true, true, "unused", "unused", 4),
            NetworkPolicy(profile.Id, PolicyApplyState.Active));
        using var services = Services(profiles, policies);
        var viewModel = services.GetRequiredService<NetworkProfilesViewModel>();

        await viewModel.DetectCommand.ExecuteAsync(null);
        await viewModel.SaveDirectCommand.ExecuteAsync(null);

        Assert.Equal("办公室", viewModel.NetworkName);
        Assert.Null(profiles.LastSavedDraft);
        Assert.Null(policies.LastSavedDraft);
        Assert.Contains("已经处于已生效", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task UnknownPolicyResultKeepsNewProfileForSafeRecovery()
    {
        var profiles = new RecordingNetworkProfileService();
        var policies = new RecordingPolicyService(new ApplyResult(
            true,
            false,
            "unused",
            "unused",
            null))
        {
            SaveException = new ControlServiceException(
                "NP_CONTROL_UNAVAILABLE",
                "控制服务连接中断。"),
        };
        using var services = Services(profiles, policies);
        var viewModel = services.GetRequiredService<NetworkProfilesViewModel>();

        await viewModel.DetectCommand.ExecuteAsync(null);
        await viewModel.SaveDirectCommand.ExecuteAsync(null);

        Assert.Single(profiles.Items);
        Assert.Equal(0, profiles.DeleteCallCount);
        Assert.Contains("无法确认规则操作结果", viewModel.OperationMessage,
            StringComparison.Ordinal);
    }

    [Fact]
    public async Task DeleteRemovesRuleBeforeReferencedProfile()
    {
        var profile = NetworkProfile("office", "办公室");
        var profiles = new RecordingNetworkProfileService(profile);
        var policies = new RecordingPolicyService(
            new ApplyResult(true, true, "unused", "unused", 4),
            NetworkPolicy(profile.Id, PolicyApplyState.Active));
        using var services = Services(profiles, policies);
        var viewModel = services.GetRequiredService<NetworkProfilesViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        await viewModel.DeleteCommand.ExecuteAsync(Assert.Single(viewModel.Items));

        Assert.Equal(1, policies.DeleteCallCount);
        Assert.Equal(1, profiles.DeleteCallCount);
        Assert.Empty(viewModel.Items);
    }

    private static ServiceProvider Services(
        RecordingNetworkProfileService profiles,
        RecordingPolicyService policies)
    {
        return TestPlatformServices.Create(configure: services =>
        {
            services.AddSingleton<ICurrentNetworkEnvironment>(
                new FixedNetworkEnvironment());
            services.AddSingleton<INetworkProfileService>(profiles);
            services.AddSingleton<IPolicyService>(policies);
        });
    }

    private static NetworkProfileListItem NetworkProfile(string id, string name)
    {
        return new NetworkProfileListItem(
            id,
            name,
            NetworkFingerprintKind.WiFiSsidSha256,
            Fingerprint,
            1);
    }

    private static PolicyListItem NetworkPolicy(
        string profileId,
        PolicyApplyState state)
    {
        return new PolicyListItem(
            "network-policy",
            "办公室直连",
            PolicyScope.Network,
            profileId,
            PolicyAction.Direct,
            state,
            4,
            DateTimeOffset.UtcNow,
            1);
    }

    private sealed class FixedNetworkEnvironment : ICurrentNetworkEnvironment
    {
        public Task<CurrentNetworkEnvironment> CaptureAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new CurrentNetworkEnvironment(
                true,
                "当前 Wi-Fi",
                NetworkFingerprintKind.WiFiSsidSha256,
                Fingerprint,
                "authorized",
                "已用本机哈希识别当前 Wi-Fi。"));
        }
    }

    private sealed class RecordingNetworkProfileService(
        params NetworkProfileListItem[] initial) : INetworkProfileService
    {
        public List<NetworkProfileListItem> Items { get; } = [.. initial];

        public NetworkProfileDraft? LastSavedDraft { get; private set; }

        public int DeleteCallCount { get; private set; }

        public Task<NetworkProfileCatalog> GetCatalogAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new NetworkProfileCatalog(
                Items.ToArray(),
                (ulong)Items.Count,
                DateTimeOffset.UtcNow));
        }

        public Task<NetworkProfileMutation> SaveAsync(
            NetworkProfileDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastSavedDraft = draft;
            var profile = new NetworkProfileListItem(
                draft.ExistingId ?? "network-created",
                draft.DisplayName.Trim(),
                draft.FingerprintKind,
                draft.FingerprintValue,
                (draft.ExistingRevision ?? 0) + 1);
            Items.RemoveAll(item => item.Id == profile.Id);
            Items.Add(profile);
            return Task.FromResult(new NetworkProfileMutation(
                true,
                "NP_NETWORK_PROFILE_SAVED",
                "已保存。",
                profile));
        }

        public Task<NetworkProfileMutation> DeleteAsync(
            string profileId,
            ulong expectedRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            DeleteCallCount++;
            Items.RemoveAll(item => item.Id == profileId);
            return Task.FromResult(new NetworkProfileMutation(
                true,
                "NP_NETWORK_PROFILE_DELETED",
                "已删除。",
                null));
        }
    }

    private sealed class RecordingPolicyService : IPolicyService
    {
        private readonly ApplyResult _saveResult;
        private readonly List<PolicyListItem> _items;

        public RecordingPolicyService(
            ApplyResult saveResult,
            params PolicyListItem[] initial)
        {
            _saveResult = saveResult;
            _items = [.. initial];
        }

        public PolicyDraft? LastSavedDraft { get; private set; }

        public int DeleteCallCount { get; private set; }

        public ControlServiceException? SaveException { get; init; }

        public Task<PolicyCatalog> GetCatalogAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new PolicyCatalog(
                _items.ToArray(),
                4,
                DateTimeOffset.UtcNow,
                _saveResult.Applied ? null : _saveResult.SnapshotVersion));
        }

        public Task<ApplyResult> SaveAsync(
            PolicyDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (SaveException is not null)
            {
                throw SaveException;
            }

            LastSavedDraft = draft;
            if (_saveResult.Accepted)
            {
                _items.RemoveAll(item => item.Id == draft.ExistingId);
                _items.Add(new PolicyListItem(
                    draft.ExistingId ?? "network-policy-created",
                    draft.Name,
                    draft.Scope,
                    draft.MatchValue,
                    draft.Action,
                    _saveResult.Applied
                        ? PolicyApplyState.Active
                        : PolicyApplyState.Pending,
                    _saveResult.SnapshotVersion,
                    DateTimeOffset.UtcNow,
                    (draft.ExistingRevision ?? 0) + 1));
            }

            return Task.FromResult(_saveResult);
        }

        public Task<ApplyResult> DeleteAsync(
            string policyId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            DeleteCallCount++;
            _items.RemoveAll(item => item.Id == policyId);
            return Task.FromResult(new ApplyResult(
                true,
                false,
                "NP_POLICY_PENDING",
                "规则删除等待确认。",
                5));
        }

        public Task<ApplyResult> RollBackAsync(
            ulong snapshotVersion,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(ApplyResult.Unavailable);
        }
    }
}
