using Google.Protobuf.WellKnownTypes;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;

namespace NonProxy.Desktop.Tests;

public sealed class GatewaySubscriptionServiceTests
{
    [Fact]
    public async Task ListReadsEveryPageAndMapsOnlySafeSourceState()
    {
        var client = new StubControlRpcClient();
        client.SubscriptionSourcesResponses.Enqueue(Page("next", Source("alpha", 1)));
        client.SubscriptionSourcesResponses.Enqueue(Page(string.Empty, Source("beta", 2)));
        var service = new GatewaySubscriptionService(client);

        var catalog = await service.ListAsync(CancellationToken.None);

        Assert.Equal(2, client.ListSubscriptionSourcesCallCount);
        Assert.Equal(["alpha", "beta"], catalog.Items.Select(item => item.Id));
        Assert.All(catalog.Items, item =>
        {
            Assert.Equal(TimeSpan.FromHours(1), item.RefreshInterval);
            Assert.Equal(3u, item.NodeCount);
            Assert.Null(item.LastErrorCode);
        });
    }

    [Fact]
    public async Task ListRejectsRepeatedCursorWithoutLoopingForever()
    {
        var client = new StubControlRpcClient();
        client.SubscriptionSourcesResponses.Enqueue(Page("repeat", Source("alpha", 1)));
        client.SubscriptionSourcesResponses.Enqueue(Page("repeat", Source("beta", 1)));
        var service = new GatewaySubscriptionService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(
            () => service.ListAsync(CancellationToken.None));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
        Assert.Equal(2, client.ListSubscriptionSourcesCallCount);
    }

    [Fact]
    public async Task CreatePassesUtf8EndpointThenClearsTheOwnedByteBuffer()
    {
        const string endpoint = "https://feed.example/nodes?token=private";
        var client = new StubControlRpcClient
        {
            UpsertSubscriptionResponse = MutationResponse(Source("office", 1)),
        };
        var service = new GatewaySubscriptionService(client);

        var result = await service.SaveAsync(
            new SubscriptionDraft(
                " office ",
                " 办公室订阅 ",
                endpoint,
                true,
                TimeSpan.FromHours(1),
                null),
            CancellationToken.None);

        Assert.True(result.Accepted);
        Assert.Equal("NP_SUBSCRIPTION_SAVED", result.Code);
        Assert.Equal("office", client.LastSubscriptionId);
        Assert.Equal("办公室订阅", client.LastSubscriptionDisplayName);
        Assert.Equal(endpoint, client.LastSubscriptionEndpointText);
        Assert.NotNull(client.LastSubscriptionEndpointBuffer);
        Assert.All(client.LastSubscriptionEndpointBuffer, value => Assert.Equal(0, value));
        Assert.Equal(0ul, client.LastSubscriptionExpectedRevision);
    }

    [Fact]
    public async Task ExistingSettingsUseEmptyEndpointAndRequireNextRevision()
    {
        var updated = Source("office", 2);
        updated.DisplayName = "办公室主订阅";
        updated.Enabled = false;
        var client = new StubControlRpcClient
        {
            UpsertSubscriptionResponse = MutationResponse(updated, contentUnchanged: true),
        };
        var service = new GatewaySubscriptionService(client);

        var result = await service.SaveAsync(
            new SubscriptionDraft(
                "office",
                "办公室主订阅",
                null,
                false,
                TimeSpan.FromHours(1),
                1),
            CancellationToken.None);

        Assert.True(result.Accepted);
        Assert.True(result.ContentUnchanged);
        Assert.Equal("NP_SUBSCRIPTION_SETTINGS_SAVED", result.Code);
        Assert.Equal(string.Empty, client.LastSubscriptionEndpointText);
        Assert.Equal(1ul, client.LastSubscriptionExpectedRevision);
        Assert.False(client.LastSubscriptionEnabled);
    }

    [Fact]
    public async Task RefreshKeepsSourceRevisionAndReportsUnchangedContent()
    {
        var client = new StubControlRpcClient
        {
            RefreshSubscriptionResponse = new RefreshSubscriptionSourceResponse
            {
                Result = new SubscriptionMutationResult
                {
                    Source = Source("office", 3),
                    ContentUnchanged = true,
                },
            },
        };
        var service = new GatewaySubscriptionService(client);

        var result = await service.RefreshAsync("office", 3, CancellationToken.None);

        Assert.True(result.Accepted);
        Assert.Equal("NP_SUBSCRIPTION_UNCHANGED", result.Code);
        Assert.Equal(3ul, result.Item?.Revision);
        Assert.Equal(3ul, client.LastSubscriptionExpectedRevision);
    }

    [Fact]
    public async Task DeleteExplainsReferencedNodesWithoutClaimingSuccess()
    {
        var client = new StubControlRpcClient
        {
            DeleteSubscriptionResponse = new DeleteSubscriptionSourceResponse
            {
                Error = new ErrorDetail
                {
                    Code = "NP_STORAGE_SUBSCRIPTION_OUTBOUND_IN_USE",
                    Message = "internal",
                },
            },
        };
        var service = new GatewaySubscriptionService(client);

        var result = await service.DeleteAsync("office", 4, CancellationToken.None);

        Assert.False(result.Accepted);
        Assert.Contains("规则或出口组", result.Message, StringComparison.Ordinal);
        Assert.Null(result.SourceId);
    }

    [Fact]
    public async Task CreateWithoutEndpointFailsBeforeCallingControlService()
    {
        var client = new StubControlRpcClient();
        var service = new GatewaySubscriptionService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(
            () => service.SaveAsync(
                new SubscriptionDraft(
                    "office",
                    "办公室订阅",
                    null,
                    true,
                    TimeSpan.FromHours(1),
                    null),
                CancellationToken.None));

        Assert.Equal("NP_REQUEST_INVALID", error.Code);
        Assert.Null(client.LastSubscriptionId);
    }

    private static ListSubscriptionSourcesResponse Page(
        string nextPageToken,
        params SubscriptionSourceSummary[] sources)
    {
        var response = new ListSubscriptionSourcesResponse
        {
            Page = new PageResponse { NextPageToken = nextPageToken },
        };
        response.Sources.Add(sources);
        return response;
    }

    private static UpsertSubscriptionSourceResponse MutationResponse(
        SubscriptionSourceSummary source,
        bool contentUnchanged = false)
    {
        return new UpsertSubscriptionSourceResponse
        {
            Result = new SubscriptionMutationResult
            {
                Source = source,
                ContentUnchanged = contentUnchanged,
            },
        };
    }

    private static SubscriptionSourceSummary Source(string id, ulong revision)
    {
        var now = DateTimeOffset.UtcNow;
        return new SubscriptionSourceSummary
        {
            Id = id,
            DisplayName = $"{id} 订阅",
            Enabled = true,
            RefreshInterval = Duration.FromTimeSpan(TimeSpan.FromHours(1)),
            Revision = revision,
            ContentGeneration = 2,
            ConsecutiveFailures = 0,
            NextRefreshAt = Timestamp.FromDateTimeOffset(now.AddHours(1)),
            LastAttemptedAt = Timestamp.FromDateTimeOffset(now),
            LastSucceededAt = Timestamp.FromDateTimeOffset(now),
            LastErrorCode = string.Empty,
            NodeCount = 3,
        };
    }
}
