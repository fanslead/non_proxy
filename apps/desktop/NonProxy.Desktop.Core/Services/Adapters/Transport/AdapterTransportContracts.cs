using Grpc.Net.Client;

namespace NonProxy.Desktop.Core.Services.Adapters.Transport;

public interface IAdapterChannelFactory
{
    GrpcChannel CreateChannel();
}

public interface IAdapterCapabilityProvider
{
    Task<byte[]> ReadAsync(CancellationToken cancellationToken);
}
