using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;
using NonProxy.Policy.V1;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayPolicyServiceTests
{
    [Fact]
    public void WebsiteDraftMapsToExactDirectPolicy()
    {
        var mapper = new PolicyContractMapper(
            new StubPlatformInformation(PlatformKind.MacOS));

        var mapped = mapper.ToContract(new PolicyDraft(
            null,
            "示例网站",
            PolicyScope.Website,
            "example.com",
            PolicyAction.Direct));

        Assert.Equal(0UL, mapped.ExpectedRevision);
        Assert.Equal(1UL, mapped.Policy.Revision);
        Assert.Equal(PolicySourceKind.Site, mapped.Policy.SourceKind);
        Assert.Equal(DomainMatchKind.Exact, mapped.Policy.Match.Domain.Kind);
        Assert.Equal("example.com", mapped.Policy.Match.Domain.AsciiPattern);
        Assert.Equal(RouteAction.Direct, mapped.Policy.Decision.Action);
    }

    [Fact]
    public void NetworkDraftMapsToProfileMatcherWithoutRawFingerprint()
    {
        var mapper = new PolicyContractMapper(
            new StubPlatformInformation(PlatformKind.MacOS));

        var mapped = mapper.ToContract(new PolicyDraft(
            null,
            "办公室直连",
            PolicyScope.Network,
            "office-network",
            PolicyAction.Direct));

        Assert.Equal(PolicySourceKind.Network, mapped.Policy.SourceKind);
        Assert.Equal("office-network", mapped.Policy.Match.Network.ProfileId);
        Assert.Null(mapped.Policy.Match.App);
        Assert.Null(mapped.Policy.Match.Domain);
    }

    [Fact]
    public void NetworkStatusIsReadableByDesktopCatalog()
    {
        var item = PolicyContractMapper.FromStatus(new PolicyStatus
        {
            Policy = new NonProxy.Policy.V1.Policy
            {
                Id = "network-direct",
                DisplayName = "办公室直连",
                SourceKind = PolicySourceKind.Network,
                Match = new PolicyMatch
                {
                    Network = new NetworkMatcher { ProfileId = "office-network" },
                },
                Decision = new DecisionSpec { Action = RouteAction.Direct },
                Revision = 1,
            },
            State = PolicyRuntimeState.Active,
            TargetSnapshotVersion = 4,
        });

        Assert.Equal(PolicyScope.Network, item.Scope);
        Assert.Equal("office-network", item.MatchValue);
        Assert.Equal(PolicyApplyState.Active, item.State);
    }

    [Fact]
    public void ExistingApplicationDraftRequiresAndIncrementsRevision()
    {
        var mapper = new PolicyContractMapper(
            new StubPlatformInformation(PlatformKind.Windows));

        var mapped = mapper.ToContract(new PolicyDraft(
            "policy-a",
            "办公应用",
            PolicyScope.Application,
            "Example.Office",
            PolicyAction.Direct,
            ExistingRevision: 7,
            ApplicationSignerId: "TEAM123",
            IncludeApplicationHelpers: true));

        Assert.Equal(7UL, mapped.ExpectedRevision);
        Assert.Equal(8UL, mapped.Policy.Revision);
        Assert.Equal(
            NonProxy.Common.V1.Platform.Windows,
            mapped.Policy.Match.App.Platform);
        Assert.Equal("Example.Office", mapped.Policy.Match.App.StableId);
        Assert.Equal("TEAM123", mapped.Policy.Match.App.SignerId);
        Assert.True(mapped.Policy.Match.App.IncludeHelpers);
    }

    [Fact]
    public async Task CatalogUsesPerPolicyRuntimeStateInsteadOfGlobalActiveFlag()
    {
        var policy = WebsitePolicy("policy-a", 3);
        var client = new StubControlRpcClient
        {
            PoliciesResponse = new ListPoliciesResponse
            {
                ActiveSnapshotVersion = 10,
                PendingSnapshotVersion = 11,
                Page = new PageResponse(),
                PolicyStatuses =
                {
                    new PolicyStatus
                    {
                        Policy = policy,
                        State = PolicyRuntimeState.PendingRemoval,
                        TargetSnapshotVersion = 11,
                        EffectiveRevision = 2,
                        PendingRevision = 3,
                    },
                },
            },
        };
        var service = Service(client);

        var catalog = await service.GetCatalogAsync(
            TestContext.Current.CancellationToken);

        var item = Assert.Single(catalog.Items);
        Assert.Equal(PolicyApplyState.PendingRemoval, item.State);
        Assert.Equal(2UL, item.EffectiveRevision);
        Assert.Equal(3UL, item.PendingRevision);
        Assert.Equal(10UL, catalog.ActiveSnapshotVersion);
        Assert.Equal(11UL, catalog.PendingSnapshotVersion);
    }

    [Fact]
    public async Task SaveReportsPendingUntilProviderAcknowledgesSnapshot()
    {
        var saved = WebsitePolicy("policy-a", 1);
        var client = new StubControlRpcClient
        {
            UpsertResponse = new UpsertPolicyResponse
            {
                Result = new PolicyMutationResult { Policy = saved },
            },
            ApplyResponse = new ApplyPolicySnapshotResponse
            {
                Result = new PolicyMutationResult
                {
                    Snapshot = new PolicySnapshotMetadata
                    {
                        SnapshotVersion = 9,
                        State = SnapshotState.PendingAck,
                    },
                },
            },
        };
        var service = Service(client);

        var result = await service.SaveAsync(
            new PolicyDraft(
                null,
                "示例网站",
                PolicyScope.Website,
                "example.com",
                PolicyAction.Direct),
            TestContext.Current.CancellationToken);

        Assert.True(result.Accepted);
        Assert.False(result.Applied);
        Assert.Equal("NP_POLICY_PENDING", result.Code);
        Assert.Equal(9UL, result.SnapshotVersion);
        Assert.NotNull(client.LastUpsertedPolicy);
    }

    [Fact]
    public async Task SaveReportsCommittedDraftWhenPublishConnectionFails()
    {
        var client = new StubControlRpcClient
        {
            UpsertResponse = new UpsertPolicyResponse
            {
                Result = new PolicyMutationResult
                {
                    Policy = WebsitePolicy("policy-a", 1),
                },
            },
            ApplyException = new ControlServiceException(
                "NP_CONTROL_UNAVAILABLE",
                "控制服务连接中断。"),
        };
        var service = Service(client);

        var result = await service.SaveAsync(
            new PolicyDraft(
                null,
                "示例网站",
                PolicyScope.Website,
                "example.com",
                PolicyAction.Direct),
            TestContext.Current.CancellationToken);

        Assert.True(result.Accepted);
        Assert.False(result.Applied);
        Assert.Equal("NP_POLICY_PUBLISH_UNKNOWN", result.Code);
        Assert.Contains("规则已保存", result.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task RejectedRollbackIsNotReportedAsAccepted()
    {
        var client = new StubControlRpcClient
        {
            RollbackResponse = new RollbackPolicySnapshotResponse
            {
                Result = new PolicyMutationResult
                {
                    Error = new ErrorDetail
                    {
                        Code = "NP_SNAPSHOT_NOT_FOUND",
                        Message = "测试错误",
                    },
                },
            },
        };
        var service = Service(client);

        var result = await service.RollBackAsync(
            99,
            TestContext.Current.CancellationToken);

        Assert.False(result.Accepted);
        Assert.False(result.Applied);
        Assert.Equal("NP_SNAPSHOT_NOT_FOUND", result.Code);
    }

    [Fact]
    public async Task CatalogRetriesWhenGenerationChangesBetweenPages()
    {
        var client = new StubControlRpcClient();
        client.PoliciesResponses.Enqueue(new ListPoliciesResponse
        {
            PolicyCatalogGeneration = 1,
            Page = new PageResponse { NextPageToken = "next" },
            PolicyStatuses =
            {
                Status(WebsitePolicy("policy-a", 1)),
            },
        });
        client.PoliciesResponses.Enqueue(new ListPoliciesResponse
        {
            PolicyCatalogGeneration = 2,
            Page = new PageResponse(),
        });
        client.PoliciesResponses.Enqueue(new ListPoliciesResponse
        {
            PolicyCatalogGeneration = 2,
            Page = new PageResponse(),
            PolicyStatuses =
            {
                Status(WebsitePolicy("policy-b", 1)),
            },
        });
        var service = Service(client);

        var catalog = await service.GetCatalogAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal("policy-b", Assert.Single(catalog.Items).Id);
        Assert.Equal(3, client.ListPoliciesCallCount);
    }

    [Fact]
    public async Task CatalogRejectsDuplicatePolicyAcrossPages()
    {
        var client = new StubControlRpcClient();
        client.PoliciesResponses.Enqueue(new ListPoliciesResponse
        {
            PolicyCatalogGeneration = 1,
            Page = new PageResponse { NextPageToken = "next" },
            PolicyStatuses =
            {
                Status(WebsitePolicy("policy-a", 1)),
            },
        });
        client.PoliciesResponses.Enqueue(new ListPoliciesResponse
        {
            PolicyCatalogGeneration = 1,
            Page = new PageResponse(),
            PolicyStatuses =
            {
                Status(WebsitePolicy("policy-a", 1)),
            },
        });
        var service = Service(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.GetCatalogAsync(TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_PAGING_INVALID", error.Code);
    }

    private static GatewayPolicyService Service(StubControlRpcClient client)
    {
        return new GatewayPolicyService(
            client,
            new PolicyContractMapper(
                new StubPlatformInformation(PlatformKind.MacOS)));
    }

    private static NonProxy.Policy.V1.Policy WebsitePolicy(
        string id,
        ulong revision)
    {
        return new NonProxy.Policy.V1.Policy
        {
            Id = id,
            DisplayName = "示例网站",
            SourceKind = PolicySourceKind.Site,
            Match = new PolicyMatch
            {
                Domain = new DomainMatcher
                {
                    Kind = DomainMatchKind.Exact,
                    AsciiPattern = "example.com",
                },
            },
            Decision = new DecisionSpec
            {
                Action = RouteAction.Direct,
                FailureMode = FailureMode.Closed,
            },
            Priority = 100,
            Enabled = true,
            Origin = PolicyOrigin.User,
            Revision = revision,
        };
    }

    private static PolicyStatus Status(NonProxy.Policy.V1.Policy policy)
    {
        return new PolicyStatus
        {
            Policy = policy,
            State = PolicyRuntimeState.Draft,
        };
    }

    private sealed record StubPlatformInformation(
        PlatformKind Platform) : IPlatformInformation
    {
        public string DisplayName => Platform.ToString();
    }
}
