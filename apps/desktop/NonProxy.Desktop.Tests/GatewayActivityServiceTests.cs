using Google.Protobuf.WellKnownTypes;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Gateway;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayActivityServiceTests
{
    [Fact]
    public async Task DirectDecisionDoesNotClaimAnObservedPath()
    {
        var client = ClientWith(new ConnectionDecisionSummary
        {
            Sequence = 7,
            ObservedAt = Timestamp.FromDateTimeOffset(
                new DateTimeOffset(2026, 7, 31, 8, 9, 10, TimeSpan.Zero)),
            AppStableId = "com.example.browser",
            AppDisplayName = "Example Browser",
            AppPlatform = Platform.Macos,
            AppSignerId = "TEAM-EXAMPLE",
            AppParentStableId = "com.example.browser",
            AppHelperGroupId = "com.example.browser",
            Destination = "example.com",
            DestinationPort = 443,
            Action = RouteAction.Direct,
            EvidenceLevel = EvidenceLevel.Decision,
            ReasonCode = "NP_POLICY_DEFAULT",
            SnapshotVersion = 3,
        });
        var service = new GatewayActivityService(client);

        var item = Assert.Single(await service.GetRecentAsync(
            20,
            TestContext.Current.CancellationToken));

        Assert.Equal("直连 · 仅策略决策", item.ResultLabel);
        Assert.Equal("尚未确认实际数据路径", item.Path);
        Assert.Equal("使用默认路由", item.Reason);
        Assert.Equal("快照 v3", item.SnapshotLabel);
        Assert.Equal(
            NonProxy.Desktop.Core.Platform.PlatformKind.MacOS,
            item.ApplicationPlatform);
        Assert.Equal("com.example.browser", item.ApplicationStableId);
        Assert.Equal("TEAM-EXAMPLE", item.ApplicationSignerId);
        Assert.Equal("com.example.browser", item.ApplicationRuleStableId);
        Assert.Equal("com.example.browser", item.ApplicationHelperGroupId);
        Assert.Equal(20, client.LastDecisionPageSize);
    }

    [Fact]
    public async Task ProxyPathAndExitEvidenceUseDifferentLabels()
    {
        var path = new ConnectionDecisionSummary
        {
            Sequence = 2,
            ObservedAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UnixEpoch),
            AppStableId = "app",
            AppPlatform = Platform.Macos,
            Destination = "api.example.com",
            DestinationPort = 443,
            Action = RouteAction.Proxy,
            EvidenceLevel = EvidenceLevel.Path,
            OutboundId = "office",
            ReasonCode = "NP_POLICY_APP_MATCH",
            MatchedPolicyId = "application-policy",
            MatchedRuleId = "app-rule",
        };
        var exit = path.Clone();
        exit.Sequence = 3;
        exit.EvidenceLevel = EvidenceLevel.Exit;
        exit.ExitProbeId = "probe-7";
        var service = new GatewayActivityService(ClientWith(path, exit));

        var items = await service.GetRecentAsync(
            20,
            TestContext.Current.CancellationToken);

        Assert.Equal("路径已确认", items[0].Evidence);
        Assert.Equal("代理出口 office", items[0].Path);
        Assert.Equal(
            "命中规则 app-rule（NP_POLICY_APP_MATCH）",
            items[0].Reason);
        Assert.Equal("公网出口已验证", items[1].Evidence);
        Assert.Equal("代理出口 office，出口探针 probe-7", items[1].Path);
    }

    [Fact]
    public async Task InvalidPathClaimIsRejectedInsteadOfDisplayed()
    {
        var service = new GatewayActivityService(ClientWith(
            new ConnectionDecisionSummary
            {
                Sequence = 1,
                ObservedAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UnixEpoch),
                AppStableId = "app",
                AppPlatform = Platform.Macos,
                Destination = "example.com",
                DestinationPort = 443,
                Action = RouteAction.Direct,
                EvidenceLevel = EvidenceLevel.Path,
                OutboundId = "proxy",
            }));

        var error = await Assert.ThrowsAsync<
            NonProxy.Desktop.Core.Services.Control.ControlServiceException>(
            () => service.GetRecentAsync(
                10,
                TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    [Fact]
    public async Task MissingApplicationPlatformIsRejectedInsteadOfGuessing()
    {
        var service = new GatewayActivityService(ClientWith(
            new ConnectionDecisionSummary
            {
                Sequence = 4,
                ObservedAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UnixEpoch),
                AppStableId = "com.example.app",
                Destination = "example.com",
                DestinationPort = 443,
                Action = RouteAction.Direct,
                EvidenceLevel = EvidenceLevel.Decision,
                ReasonCode = "NP_POLICY_DEFAULT",
            }));

        var error = await Assert.ThrowsAsync<
            NonProxy.Desktop.Core.Services.Control.ControlServiceException>(
            () => service.GetRecentAsync(
                10,
                TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    [Fact]
    public async Task FailOpenProxyShowsTheObservedDirectFallback()
    {
        var service = new GatewayActivityService(ClientWith(
            new ConnectionDecisionSummary
            {
                Sequence = 9,
                ObservedAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UnixEpoch),
                AppStableId = "app",
                AppPlatform = Platform.Macos,
                Destination = "example.com",
                DestinationPort = 443,
                Action = RouteAction.Proxy,
                FailureMode = FailureMode.Open,
                EvidenceLevel = EvidenceLevel.Path,
                InterfaceName = "ifindex:12",
                FailOpenDirect = true,
                ErrorCode = "NP_PROXY_FAIL_OPEN_DIRECT",
            }));

        var item = Assert.Single(await service.GetRecentAsync(
            10,
            TestContext.Current.CancellationToken));

        Assert.Equal("代理失败→直连 · 路径已确认", item.ResultLabel);
        Assert.Equal("物理接口 ifindex:12（fail-open 回退）", item.Path);
        Assert.Equal("NP_PROXY_FAIL_OPEN_DIRECT", item.Error);
    }

    private static StubControlRpcClient ClientWith(
        params ConnectionDecisionSummary[] decisions)
    {
        var response = new ListConnectionDecisionsResponse
        {
            Page = new PageResponse(),
            TotalCount = checked((ulong)decisions.Length),
        };
        response.Decisions.AddRange(decisions);
        return new StubControlRpcClient { DecisionsResponse = response };
    }
}
