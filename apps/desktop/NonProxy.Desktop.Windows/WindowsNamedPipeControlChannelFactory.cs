using Grpc.Net.Client;
using NonProxy.Desktop.Core.Services.Control.Transport;

namespace NonProxy.Desktop.Windows;

internal sealed class WindowsNamedPipeControlChannelFactory : IControlChannelFactory
{
    private readonly LocalControlEndpoint _endpoint;

    public WindowsNamedPipeControlChannelFactory(LocalControlEndpoint endpoint)
    {
        _endpoint = endpoint;
    }

    public GrpcChannel CreateChannel()
    {
        return LocalNamedPipeGrpcChannel.Create(
            _endpoint.IsConfigured ? _endpoint.NamedPipePath : null,
            "NP_CONTROL_UNAVAILABLE",
            "Windows 本地控制管道尚未配置。");
    }
}
