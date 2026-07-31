using Grpc.Net.Client;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Services.Adapters.Transport;

public sealed class UnavailableAdapterChannelFactory : IAdapterChannelFactory
{
    public GrpcChannel CreateChannel()
    {
        throw new ControlServiceException(
            "NP_ADAPTER_UNAVAILABLE",
            "当前平台尚未配置第三方客户端适配器传输。");
    }
}

public sealed class UnavailableAdapterCapabilityProvider : IAdapterCapabilityProvider
{
    public Task<byte[]> ReadAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        throw new ControlServiceException(
            "NP_ADAPTER_UNAVAILABLE",
            "当前平台尚未配置第三方客户端适配器会话。");
    }
}
