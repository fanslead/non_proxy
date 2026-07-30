using System.Text.Json;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayOutboundServiceTests
{
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
}
