using Grpc.Net.Client;

namespace NonProxy.Desktop.Core.Services.Control.Transport;

public interface IControlChannelFactory
{
    GrpcChannel CreateChannel();
}

public interface ISessionCapabilityProvider
{
    Task<byte[]> ReadAsync(CancellationToken cancellationToken);
}
