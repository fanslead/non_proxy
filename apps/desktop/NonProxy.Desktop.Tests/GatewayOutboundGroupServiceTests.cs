using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;
using NonProxy.Policy.V1;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayOutboundGroupServiceTests
{
    [Fact]
    public async Task ListPreservesPriorityAndFindsDefaultAcrossPages()
    {
        var client = new StubControlRpcClient();
        client.OutboundGroupsResponses.Enqueue(new ListOutboundGroupsResponse
        {
            RoutingRevision = 7,
            Page = new PageResponse { NextPageToken = "next" },
            Groups = { Group("backup", "Backup", false, 2, "b", "c") },
        });
        client.OutboundGroupsResponses.Enqueue(new ListOutboundGroupsResponse
        {
            RoutingRevision = 7,
            Page = new PageResponse(),
            Groups = { Group("office", "Office", true, 4, "a", "b") },
        });
        var service = new GatewayOutboundGroupService(client);

        var catalog = await service.ListAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal<ulong>(7, catalog.RoutingRevision);
        Assert.Equal("office", catalog.DefaultGroupId);
        Assert.Equal(["a", "b"], catalog.Groups[1].OutboundIds);
    }

    [Fact]
    public async Task ListRejectsRoutingRevisionDriftAcrossPages()
    {
        var client = new StubControlRpcClient();
        client.OutboundGroupsResponses.Enqueue(new ListOutboundGroupsResponse
        {
            RoutingRevision = 7,
            Page = new PageResponse { NextPageToken = "next" },
            Groups = { Group("one", "One", false, 1, "a", "b") },
        });
        client.OutboundGroupsResponses.Enqueue(new ListOutboundGroupsResponse
        {
            RoutingRevision = 8,
            Page = new PageResponse(),
            Groups = { Group("two", "Two", false, 1, "b", "c") },
        });
        var service = new GatewayOutboundGroupService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.ListAsync(TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    [Fact]
    public async Task ListRejectsDuplicateMember()
    {
        var client = new StubControlRpcClient
        {
            OutboundGroupsResponse = new ListOutboundGroupsResponse
            {
                RoutingRevision = 1,
                Page = new PageResponse(),
                Groups = { Group("bad", "Bad", false, 1, "a", "a") },
            },
        };
        var service = new GatewayOutboundGroupService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.ListAsync(TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    [Fact]
    public async Task SaveNewGroupSendsOrderedMembersAndRevisionZero()
    {
        var client = new StubControlRpcClient
        {
            UpsertOutboundGroupResponse = new UpsertOutboundGroupResponse
            {
                Result = new OutboundGroupMutationResult
                {
                    Group = Group("office", "Office", false, 1, "primary", "backup"),
                    RoutingRevision = 3,
                },
            },
        };
        var service = new GatewayOutboundGroupService(client);

        var result = await service.SaveAsync(
            new OutboundGroupDraft(
                "office",
                "Office",
                ["primary", "backup"]),
            TestContext.Current.CancellationToken);

        Assert.True(result.Accepted);
        Assert.Equal<ulong>(0, client.LastExpectedRevision);
        Assert.Equal(["primary", "backup"], client.LastOutboundGroupMembers);
        Assert.Null(result.PendingSnapshotVersion);
    }

    [Fact]
    public async Task SavingDefaultGroupRequiresPendingSnapshot()
    {
        var client = new StubControlRpcClient
        {
            UpsertOutboundGroupResponse = new UpsertOutboundGroupResponse
            {
                Result = new OutboundGroupMutationResult
                {
                    Group = Group("office", "Office", true, 3, "backup", "primary"),
                    RoutingRevision = 9,
                },
            },
        };
        var service = new GatewayOutboundGroupService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.SaveAsync(
                new OutboundGroupDraft(
                    "office",
                    "Office",
                    ["backup", "primary"],
                    2),
                TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    [Fact]
    public async Task SavingDefaultGroupReportsPendingSnapshot()
    {
        var client = new StubControlRpcClient
        {
            UpsertOutboundGroupResponse = new UpsertOutboundGroupResponse
            {
                Result = new OutboundGroupMutationResult
                {
                    Group = Group("office", "Office", true, 3, "backup", "primary"),
                    RoutingRevision = 9,
                    Snapshot = new PolicySnapshotMetadata
                    {
                        SnapshotVersion = 12,
                        State = SnapshotState.PendingAck,
                    },
                },
            },
        };
        var service = new GatewayOutboundGroupService(client);

        var result = await service.SaveAsync(
            new OutboundGroupDraft(
                "office",
                "Office",
                ["backup", "primary"],
                2),
            TestContext.Current.CancellationToken);

        Assert.True(result.Accepted);
        Assert.Equal((ulong?)12, result.PendingSnapshotVersion);
        Assert.Contains("等待系统组件确认", result.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task DeleteMapsInUseWithoutClaimingDeletion()
    {
        var client = new StubControlRpcClient
        {
            DeleteOutboundGroupResponse = new DeleteOutboundGroupResponse
            {
                GroupId = "office",
                Error = new ErrorDetail
                {
                    Code = "NP_STORAGE_OUTBOUND_GROUP_IN_USE",
                },
            },
        };
        var service = new GatewayOutboundGroupService(client);

        var result = await service.DeleteAsync(
            "office",
            4,
            TestContext.Current.CancellationToken);

        Assert.False(result.Accepted);
        Assert.Contains("默认路由或规则", result.Message, StringComparison.Ordinal);
        Assert.Equal<ulong>(4, client.LastExpectedRevision);
    }

    [Fact]
    public async Task SetDefaultUsesGroupRouteAndReportsPendingActivation()
    {
        var client = new StubControlRpcClient
        {
            SetDefaultRouteResponse = new SetDefaultRouteResponse
            {
                RoutingRevision = 6,
                Snapshot = new PolicySnapshotMetadata
                {
                    SnapshotVersion = 18,
                    State = SnapshotState.PendingAck,
                },
            },
        };
        var service = new GatewayOutboundGroupService(client);

        var result = await service.SetDefaultAsync(
            "office",
            5,
            TestContext.Current.CancellationToken);

        Assert.True(result.Accepted);
        Assert.Equal("office", client.LastDefaultOutboundGroupId);
        Assert.Equal<ulong>(5, client.LastExpectedRoutingRevision);
        Assert.Equal((ulong?)18, result.SnapshotVersion);
    }

    private static OutboundGroupSummary Group(
        string id,
        string name,
        bool isDefault,
        ulong revision,
        params string[] members)
    {
        var group = new OutboundGroupSummary
        {
            Id = id,
            DisplayName = name,
            Strategy = OutboundGroupStrategy.Failover,
            Revision = revision,
            IsDefault = isDefault,
        };
        group.OutboundIds.Add(members);
        return group;
    }
}
