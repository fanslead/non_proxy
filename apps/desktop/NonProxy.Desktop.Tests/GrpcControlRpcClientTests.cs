using Google.Protobuf;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Tests;

public sealed class GrpcControlRpcClientTests
{
    [Fact]
    public void TestOutboundBuildsAuthenticatedBoundedRequest()
    {
        var context = new OperationContext
        {
            OperationId = "desktop:test-outbound:operation",
            SessionCapabilityToken = ByteString.CopyFrom(Enumerable.Range(0, 32)
                .Select(value => (byte)value)
                .ToArray()),
        };

        var request = GrpcControlRpcClient.CreateTestOutboundRequest(
            "office",
            context);

        Assert.Same(context, request.Context);
        Assert.Equal("office", request.OutboundId);
        Assert.NotNull(request.Timeout);
        Assert.Equal(5, request.Timeout.Seconds);
        Assert.Equal(0, request.Timeout.Nanos);
        Assert.Equal(32, request.Context.SessionCapabilityToken.Length);
    }
}
