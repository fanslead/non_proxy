using System.IO.Pipes;
using Grpc.Net.Client;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Windows;

internal static class LocalNamedPipeGrpcChannel
{
    private const string LocalPipePrefix = @"\\.\pipe\";
    private const string ProductPipePrefix = @"\\.\pipe\NonProxy.";
    private const int MaximumMessageBytes = 4 * 1024 * 1024;

    public static GrpcChannel Create(
        string? pipePath,
        string unavailableCode,
        string unavailableMessage)
    {
        if (!IsValidPipePath(pipePath))
        {
            throw new ControlServiceException(
                unavailableCode,
                unavailableMessage);
        }

        var handler = new SocketsHttpHandler
        {
            ConnectCallback = (_, cancellationToken) =>
                ConnectAsync(pipePath!, cancellationToken),
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

    private static bool IsValidPipePath(string? pipePath)
    {
        if (string.IsNullOrWhiteSpace(pipePath)
            || !pipePath.StartsWith(ProductPipePrefix, StringComparison.Ordinal))
        {
            return false;
        }

        var suffix = pipePath[ProductPipePrefix.Length..];
        return pipePath.Length <= 160
            && suffix.Length > 0
            && suffix.All(character =>
                char.IsAsciiLetterOrDigit(character)
                || character is '.' or '_' or '-');
    }

    private static async ValueTask<Stream> ConnectAsync(
        string pipePath,
        CancellationToken cancellationToken)
    {
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
