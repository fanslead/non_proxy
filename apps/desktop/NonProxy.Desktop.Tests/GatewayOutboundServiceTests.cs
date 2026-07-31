using System.Globalization;
using System.Text.Json;
using Google.Protobuf.WellKnownTypes;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayOutboundServiceTests
{
    [Fact]
    public async Task ListMapsFreshHealthObservation()
    {
        var checkedAt = DateTimeOffset.Parse(
            "2026-07-31T01:02:03Z",
            CultureInfo.InvariantCulture);
        var client = new StubControlRpcClient
        {
            OutboundsResponse = new ListOutboundsResponse
            {
                Page = new PageResponse(),
                Outbounds =
                {
                    new OutboundSummary
                    {
                        Id = "office",
                        DisplayName = "Office proxy",
                        Kind = OutboundKind.HttpConnect,
                        EndpointHost = "127.0.0.1",
                        EndpointPort = 8_080,
                        Health = NonProxy.Events.V1.RuntimeState.Ready,
                        LastCheckedAt = Timestamp.FromDateTimeOffset(checkedAt),
                        Latency = Duration.FromTimeSpan(TimeSpan.FromMilliseconds(42)),
                    },
                },
            },
        };
        var service = new GatewayOutboundService(client);

        var item = Assert.Single(await service.ListAsync(
            TestContext.Current.CancellationToken));

        Assert.Equal("代理握手可用", item.Health);
        Assert.Equal(TimeSpan.FromMilliseconds(42), item.Latency);
        Assert.Equal(checkedAt, item.LastCheckedAt);
    }

    [Fact]
    public async Task ImportBuildsStrictVersionedConfiguration()
    {
        var client = new StubControlRpcClient
        {
            ImportResponse = new ImportConfigurationResponse
            {
                ImportId = "import-1",
                Outbounds =
                {
                    new OutboundSummary
                    {
                        Id = "office",
                        DisplayName = "office",
                        Kind = OutboundKind.Socks5,
                        EndpointHost = "127.0.0.1",
                        EndpointPort = 1080,
                    },
                },
            },
        };
        var service = new GatewayOutboundService(client);

        var result = await service.ImportAsync(
            new OutboundImportDraft(
                "office",
                OutboundProxyKind.Socks5,
                "127.0.0.1",
                1080,
                "alice",
                "private"),
            TestContext.Current.CancellationToken);

        Assert.Equal("import-1", result.ImportId);
        Assert.Equal("127.0.0.1:1080", Assert.Single(result.Outbounds).Endpoint);
        Assert.NotNull(client.LastImportedConfiguration);
        using var document = JsonDocument.Parse(client.LastImportedConfiguration);
        Assert.Equal(1, document.RootElement.GetProperty("version").GetInt32());
        var outbound = document.RootElement
            .GetProperty("outbounds")
            .EnumerateArray()
            .Single();
        Assert.Equal("socks5", outbound.GetProperty("kind").GetString());
        Assert.Equal("private", outbound.GetProperty("password").GetString());
    }

    [Fact]
    public async Task ImportMapsCredentialStoreFailureToActionableMessage()
    {
        var client = new StubControlRpcClient
        {
            ImportResponse = new ImportConfigurationResponse
            {
                Error = new ErrorDetail
                {
                    Code = "NP_CREDENTIAL_STORE_FAILED",
                },
                Warnings =
                {
                    "代理凭据未能全部清理。",
                },
            },
        };
        var service = new GatewayOutboundService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.ImportAsync(
                new OutboundImportDraft(
                    "office",
                    OutboundProxyKind.Socks5,
                    "127.0.0.1",
                    1080,
                    "alice",
                    "private"),
                TestContext.Current.CancellationToken));

        Assert.Equal("NP_CREDENTIAL_STORE_FAILED", error.Code);
        Assert.Contains("系统凭据库", error.UserMessage, StringComparison.Ordinal);
        Assert.Contains("未能全部清理", error.UserMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task TestMapsHandshakeFailureToActionableMessage()
    {
        var client = new StubControlRpcClient
        {
            TestOutboundResponse = new TestOutboundResponse
            {
                Error = new ErrorDetail
                {
                    Code = "NP_FLOW_OUTBOUND_CONNECT_FAILED",
                    Retryable = true,
                },
            },
        };
        var service = new GatewayOutboundService(client);

        var result = await service.TestAsync(
            "office",
            TestContext.Current.CancellationToken);

        Assert.Equal("office", client.LastTestedOutboundId);
        Assert.False(result.Healthy);
        Assert.Equal("握手异常", result.Health);
        Assert.Null(result.Latency);
        Assert.Contains("认证信息", result.Message, StringComparison.Ordinal);
        Assert.DoesNotContain("公网出口", result.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task TestMapsSuccessfulHandshakeWithoutOverclaimingEgress()
    {
        var client = new StubControlRpcClient
        {
            TestOutboundResponse = new TestOutboundResponse
            {
                Healthy = true,
                Latency = Duration.FromTimeSpan(TimeSpan.FromMilliseconds(37)),
            },
        };
        var service = new GatewayOutboundService(client);

        var result = await service.TestAsync(
            "office",
            TestContext.Current.CancellationToken);

        Assert.True(result.Healthy);
        Assert.Equal("代理握手可用", result.Health);
        Assert.Equal(TimeSpan.FromMilliseconds(37), result.Latency);
        Assert.Contains("不代表公网出口", result.Message, StringComparison.Ordinal);
        Assert.Contains("最终规则路径", result.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task TestRejectsHealthyResponseWithoutLatency()
    {
        var client = new StubControlRpcClient
        {
            TestOutboundResponse = new TestOutboundResponse { Healthy = true },
        };
        var service = new GatewayOutboundService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.TestAsync(
                "office",
                TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    [Fact]
    public async Task TestRejectsLatencyAboveServerMaximum()
    {
        var client = new StubControlRpcClient
        {
            TestOutboundResponse = new TestOutboundResponse
            {
                Healthy = true,
                Latency = Duration.FromTimeSpan(TimeSpan.FromSeconds(31)),
            },
        };
        var service = new GatewayOutboundService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.TestAsync(
                "office",
                TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }
}
