using Grpc.Net.Client;
using NonProxy.Desktop.Core.Services.Adapters.Transport;

namespace NonProxy.Desktop.Windows;

internal sealed class WindowsNamedPipeAdapterChannelFactory(
    LocalAdapterEndpoint endpoint) : IAdapterChannelFactory
{
    public GrpcChannel CreateChannel()
    {
        return LocalNamedPipeGrpcChannel.Create(
            endpoint.IsConfigured ? endpoint.NamedPipePath : null,
            "NP_ADAPTER_UNAVAILABLE",
            "Windows 本地 Adapter 管道尚未配置。");
    }
}
