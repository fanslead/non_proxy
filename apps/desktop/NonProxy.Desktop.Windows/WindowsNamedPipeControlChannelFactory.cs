using System.IO.Pipes;
using Grpc.Net.Client;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Transport;

namespace NonProxy.Desktop.Windows;

internal sealed class WindowsNamedPipeControlChannelFactory : IControlChannelFactory
{
    private const string LocalPipePrefix = @"\\.\pipe\";
    private const string ProductPipePrefix = @"\\.\pipe\NonProxy.";
    private const int MaximumMessageBytes = 4 * 1024 * 1024;
    private readonly LocalControlEndpoint _endpoint;

    public WindowsNamedPipeControlChannelFactory(LocalControlEndpoint endpoint)
    {
        _endpoint = endpoint;
    }

    public GrpcChannel CreateChannel()
    {
        if (!_endpoint.IsConfigured
            || string.IsNullOrWhiteSpace(_endpoint.NamedPipePath))
        {
            throw new ControlServiceException(
                "NP_CONTROL_UNAVAILABLE",
                "Windows 本地控制管道尚未配置。");
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
        var pipePath = _endpoint.NamedPipePath;
        if (string.IsNullOrWhiteSpace(pipePath)
            || !pipePath.StartsWith(ProductPipePrefix, StringComparison.Ordinal))
        {
            throw new IOException("Windows 本地控制管道路径无效。");
        }

        var stream = new NamedPipeClientStream(
            ".",
            pipePath[LocalPipePrefix.Length..],
            PipeDirection.InOut,
            PipeOptions.Asynchronous | PipeOptions.WriteThrough);
        try
        {
            await stream.ConnectAsync(cancellationToken);
            return stream;
        }
        catch
        {
            await stream.DisposeAsync();
            throw;
        }
    }
}
