using Google.Protobuf;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Transport;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed class OperationContextProvider
{
    private readonly ISessionCapabilityProvider _capabilityProvider;

    public OperationContextProvider(ISessionCapabilityProvider capabilityProvider)
    {
        _capabilityProvider = capabilityProvider;
    }

    public async Task<OperationContext> CreateAsync(
        string operation,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(operation);
        var token = await _capabilityProvider.ReadAsync(cancellationToken);
        return new OperationContext
        {
            OperationId = $"desktop:{operation}:{Guid.NewGuid():N}",
            SessionCapabilityToken = ByteString.CopyFrom(token),
        };
    }
}
