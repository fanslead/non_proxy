using Grpc.Net.Client;

namespace NonProxy.Desktop.Core.Services.Control.Transport;

public sealed class UnavailableControlChannelFactory : IControlChannelFactory
{
    public GrpcChannel CreateChannel()
    {
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "当前平台尚未配置本地控制传输。");
    }
}

public sealed class UnavailableSessionCapabilityProvider : ISessionCapabilityProvider
{
    public Task<byte[]> ReadAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_CONTROL_UNAVAILABLE",
            "当前平台尚未配置本地会话能力。");
    }
}
