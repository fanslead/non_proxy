using System.Net;
using System.Net.Sockets;
using Grpc.Net.Client;

namespace NonProxy.Desktop.Core.Services.Control.Transport;

public sealed class UnixDomainSocketControlChannelFactory : IControlChannelFactory
{
    private const int MaximumMessageBytes = 4 * 1024 * 1024;

    private readonly LocalControlEndpoint _endpoint;

    public UnixDomainSocketControlChannelFactory(LocalControlEndpoint endpoint)
    {
        _endpoint = endpoint;
    }

    public GrpcChannel CreateChannel()
    {
        if (!_endpoint.IsConfigured || string.IsNullOrWhiteSpace(_endpoint.SocketPath))
        {
            throw new ControlServiceException(
                "NP_CONTROL_UNAVAILABLE",
                "本地控制套接字尚未配置。");
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
        var socketPath = _endpoint.SocketPath;
        if (string.IsNullOrWhiteSpace(socketPath))
        {
            throw new IOException("本地控制套接字路径无效。");
        }

        var socket = new Socket(
            AddressFamily.Unix,
            SocketType.Stream,
            ProtocolType.Unspecified);
        try
        {
            var endpoint = new UnixDomainSocketEndPoint(socketPath);
            await socket.ConnectAsync(endpoint, cancellationToken);
            return new NetworkStream(socket, ownsSocket: true);
        }
        catch
        {
            socket.Dispose();
            throw;
        }
    }
}
