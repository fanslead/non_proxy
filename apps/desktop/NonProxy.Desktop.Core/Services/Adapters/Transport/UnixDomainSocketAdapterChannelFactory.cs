using System.Net;
using System.Net.Sockets;
using Grpc.Net.Client;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Services.Adapters.Transport;

public sealed class UnixDomainSocketAdapterChannelFactory(
    LocalAdapterEndpoint endpoint) : IAdapterChannelFactory
{
    private const int MaximumMessageBytes = 4 * 1024 * 1024;

    public GrpcChannel CreateChannel()
    {
        if (!endpoint.IsConfigured
            || string.IsNullOrWhiteSpace(endpoint.SocketPath))
        {
            throw new ControlServiceException(
                "NP_ADAPTER_UNAVAILABLE",
                "本地适配器套接字尚未配置。");
        }

        var handler = new SocketsHttpHandler
        {
            ConnectCallback = ConnectAsync,
            EnableMultipleHttp2Connections = false,
            PooledConnectionIdleTimeout = TimeSpan.FromMinutes(2),
        };
        return GrpcChannel.ForAddress(
            "http://localhost",
            new GrpcChannelOptions
            {
                HttpHandler = handler,
                MaxReceiveMessageSize = MaximumMessageBytes,
                MaxSendMessageSize = MaximumMessageBytes,
            });
    }

    private async ValueTask<Stream> ConnectAsync(
        SocketsHttpConnectionContext context,
        CancellationToken cancellationToken)
    {
        _ = context;
        if (string.IsNullOrWhiteSpace(endpoint.SocketPath))
        {
            throw new IOException("本地适配器套接字路径无效。");
        }

        var socket = new Socket(
            AddressFamily.Unix,
            SocketType.Stream,
            ProtocolType.Unspecified);
        try
        {
            await socket.ConnectAsync(
                new UnixDomainSocketEndPoint(endpoint.SocketPath),
                cancellationToken);
            return new NetworkStream(socket, ownsSocket: true);
        }
        catch
        {
            socket.Dispose();
            throw;
        }
    }
}
