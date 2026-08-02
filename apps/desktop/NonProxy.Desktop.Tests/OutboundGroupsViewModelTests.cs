using NonProxy.Desktop.Core.Features.Outbounds;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class OutboundGroupsViewModelTests
{
    [Fact]
    public async Task CreatePreservesReorderedPriorityWithoutExposingGroupId()
    {
        var service = new RecordingOutboundGroupService();
        var viewModel = new OutboundGroupsViewModel(service);
        var outbounds = EligibleOutbounds();
        viewModel.ApplyCatalog(new OutboundGroupCatalog([], 3), outbounds);

        viewModel.StartCreateCommand.Execute(null);
        viewModel.AddMemberCommand.Execute(outbounds[0]);
        viewModel.AddMemberCommand.Execute(outbounds[1]);
        viewModel.MoveMemberUpCommand.Execute(viewModel.PriorityMembers[1]);

        Assert.Equal(
            ["backup", "primary"],
            viewModel.PriorityMembers.Select(member => member.OutboundId));
        Assert.True(viewModel.SaveCommand.CanExecute(null));

        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.NotNull(service.LastDraft);
        Assert.StartsWith("failover-", service.LastDraft.Id, StringComparison.Ordinal);
        Assert.Equal(["backup", "primary"], service.LastDraft.OutboundIds);
        Assert.False(viewModel.IsEditorVisible);
        Assert.Single(viewModel.Groups);
    }

    [Fact]
    public void SaveStaysDisabledUntilTwoUniqueMembersAreSelected()
    {
        var service = new RecordingOutboundGroupService();
        var viewModel = new OutboundGroupsViewModel(service);
        var outbounds = EligibleOutbounds();
        viewModel.ApplyCatalog(new OutboundGroupCatalog([], 3), outbounds);

        viewModel.StartCreateCommand.Execute(null);
        viewModel.AddMemberCommand.Execute(outbounds[0]);

        Assert.False(viewModel.AddMemberCommand.CanExecute(outbounds[0]));
        Assert.False(viewModel.SaveCommand.CanExecute(null));
    }

    [Fact]
    public async Task SettingDefaultUpdatesAllGroupsAndPublishesRoutingRevision()
    {
        var service = new RecordingOutboundGroupService();
        var viewModel = new OutboundGroupsViewModel(service);
        var office = new OutboundGroupListItem(
            "office",
            "Office",
            ["primary", "backup"],
            2);
        var home = new OutboundGroupListItem(
            "home",
            "Home",
            ["backup", "primary"],
            1,
            IsDefault: true);
        viewModel.ApplyCatalog(
            new OutboundGroupCatalog([office, home], 7, "home"),
            EligibleOutbounds());
        Assert.Equal(
            "Primary  →  Backup",
            viewModel.Groups.Single(group => group.Id == "office").PrioritySummary);
        OutboundGroupDefaultRouteChange? change = null;
        viewModel.DefaultRouteChanged += (_, value) => change = value;

        await viewModel.SetDefaultCommand.ExecuteAsync(
            viewModel.Groups.Single(group => group.Id == "office"));

        Assert.Equal("office", service.LastDefaultGroupId);
        Assert.Equal<ulong>(7, service.LastExpectedRoutingRevision);
        Assert.True(viewModel.Groups.Single(group => group.Id == "office").IsDefault);
        Assert.False(viewModel.Groups.Single(group => group.Id == "home").IsDefault);
        Assert.Equal((ulong?)8, change?.RoutingRevision);
    }

    [Fact]
    public async Task DeleteRequiresConfirmationAndUsesCurrentRevision()
    {
        var service = new RecordingOutboundGroupService();
        var viewModel = new OutboundGroupsViewModel(service);
        var group = new OutboundGroupListItem(
            "office",
            "Office",
            ["primary", "backup"],
            5);
        viewModel.ApplyCatalog(
            new OutboundGroupCatalog([group], 7),
            EligibleOutbounds());

        viewModel.RequestDeleteCommand.Execute(Assert.Single(viewModel.Groups));
        Assert.True(viewModel.IsDeleteConfirmationVisible);

        await viewModel.ConfirmDeleteCommand.ExecuteAsync(null);

        Assert.Equal("office", service.LastDeletedGroupId);
        Assert.Equal<ulong>(5, service.LastExpectedRevision);
        Assert.Empty(viewModel.Groups);
    }

    private static IReadOnlyList<OutboundListItem> EligibleOutbounds()
    {
        return
        [
            new OutboundListItem(
                "primary",
                "Primary",
                "SOCKS5",
                "127.0.0.1:1080",
                "代理握手可用",
                TimeSpan.FromMilliseconds(20),
                DateTimeOffset.UtcNow,
                SupportsDefaultRoute: true,
                IsHandshakeVerified: true),
            new OutboundListItem(
                "backup",
                "Backup",
                "Shadowsocks",
                "proxy.example:8388",
                "代理握手可用",
                TimeSpan.FromMilliseconds(30),
                DateTimeOffset.UtcNow,
                SupportsDefaultRoute: true,
                IsHandshakeVerified: true),
        ];
    }

    private sealed class RecordingOutboundGroupService : IOutboundGroupService
    {
        public OutboundGroupDraft? LastDraft { get; private set; }

        public string? LastDefaultGroupId { get; private set; }

        public string? LastDeletedGroupId { get; private set; }

        public ulong LastExpectedRoutingRevision { get; private set; }

        public ulong LastExpectedRevision { get; private set; }

        public Task<OutboundGroupCatalog> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new OutboundGroupCatalog([], 1));
        }

        public Task<OutboundGroupMutation> SaveAsync(
            OutboundGroupDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastDraft = draft;
            var group = new OutboundGroupListItem(
                draft.Id,
                draft.Name,
                draft.OutboundIds,
                (draft.ExpectedRevision ?? 0) + 1);
            return Task.FromResult(new OutboundGroupMutation(
                true,
                "NP_OUTBOUND_GROUP_SAVED",
                "自动切换线路组已保存。",
                group,
                3));
        }

        public Task<OutboundGroupDeletion> DeleteAsync(
            string groupId,
            ulong expectedRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastDeletedGroupId = groupId;
            LastExpectedRevision = expectedRevision;
            return Task.FromResult(new OutboundGroupDeletion(
                true,
                "NP_OUTBOUND_GROUP_DELETED",
                "自动切换线路组已删除。"));
        }

        public Task<ApplyResult> SetDefaultAsync(
            string groupId,
            ulong expectedRoutingRevision,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LastDefaultGroupId = groupId;
            LastExpectedRoutingRevision = expectedRoutingRevision;
            return Task.FromResult(new ApplyResult(
                true,
                false,
                "NP_SNAPSHOT_PENDING_ACK",
                "等待系统组件确认。",
                8));
        }
    }
}
