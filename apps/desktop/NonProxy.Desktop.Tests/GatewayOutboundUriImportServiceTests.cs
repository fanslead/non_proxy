using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayOutboundUriImportServiceTests
{
    [Fact]
    public async Task PreviewUsesValidationOnlyAndReturnsSecretFreeSummaries()
    {
        const string source = "socks5://alice:private@proxy.example:1080#office";
        var client = new StubControlRpcClient
        {
            ImportResponse = Response(),
        };
        var service = new GatewayOutboundService(client);

        var result = await service.PreviewUriListAsync(
            source,
            TestContext.Current.CancellationToken);

        Assert.Equal("proxy-uri-list-v1", client.LastImportFormat);
        Assert.True(client.LastImportWasValidationOnly);
        Assert.Equal(source, client.LastImportedConfiguration);
        var outbound = Assert.Single(result.Outbounds);
        Assert.Equal("office", outbound.Id);
        Assert.DoesNotContain("alice", outbound.Endpoint, StringComparison.Ordinal);
        Assert.DoesNotContain("private", outbound.Endpoint, StringComparison.Ordinal);
    }

    [Fact]
    public async Task SaveUsesTheSameFormatWithoutValidationOnly()
    {
        var client = new StubControlRpcClient
        {
            ImportResponse = Response(),
        };
        var service = new GatewayOutboundService(client);

        await service.ImportUriListAsync(
            "http://proxy.example:8080#office",
            TestContext.Current.CancellationToken);

        Assert.Equal("proxy-uri-list-v1", client.LastImportFormat);
        Assert.False(client.LastImportWasValidationOnly);
    }

    [Fact]
    public async Task UnsupportedSchemeMapsToActionableMessageWithoutEchoingInput()
    {
        var client = new StubControlRpcClient
        {
            ImportResponse = new ImportConfigurationResponse
            {
                Error = new ErrorDetail
                {
                    Code = "NP_OUTBOUND_IMPORT_URI_SCHEME_UNSUPPORTED",
                    Metadata = { ["line"] = "2" },
                },
            },
        };
        var service = new GatewayOutboundService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.PreviewUriListAsync(
                "https://alice:private@secret.example:443",
                TestContext.Current.CancellationToken));

        Assert.Contains("socks5://", error.UserMessage, StringComparison.Ordinal);
        Assert.Contains("第 2 行", error.UserMessage, StringComparison.Ordinal);
        Assert.DoesNotContain("alice", error.UserMessage, StringComparison.Ordinal);
        Assert.DoesNotContain("private", error.UserMessage, StringComparison.Ordinal);
        Assert.DoesNotContain("secret.example", error.UserMessage, StringComparison.Ordinal);
    }

    private static ImportConfigurationResponse Response()
    {
        return new ImportConfigurationResponse
        {
            ImportId = "import-1",
            Outbounds =
            {
                new OutboundSummary
                {
                    Id = "office",
                    DisplayName = "office",
                    Kind = OutboundKind.Socks5,
                    EndpointHost = "proxy.example",
                    EndpointPort = 1080,
                },
            },
        };
    }
}
