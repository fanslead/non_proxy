using System.Security.Cryptography;
using Google.Protobuf;
using NonProxy.Adapter.V1;
using NonProxy.Desktop.Core.Services.Adapters.Transport;

namespace NonProxy.Desktop.Core.Services.Adapters.Rpc;

public sealed class AdapterRequestContextProvider(
    IAdapterCapabilityProvider capabilityProvider)
{
    public async Task<AdapterRequestContext> CreateAsync(
        string operation,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(operation);
        var token = await capabilityProvider.ReadAsync(cancellationToken);
        try
        {
            if (token.Length != FileAdapterCapabilityProvider.TokenLength)
            {
                throw new InvalidOperationException(
                    "适配器会话能力长度无效。");
            }
            return new AdapterRequestContext
            {
                OperationId = $"desktop:{operation}:{Guid.NewGuid():N}",
                SessionCapabilityToken = ByteString.CopyFrom(token),
            };
        }
        finally
        {
            CryptographicOperations.ZeroMemory(token);
        }
    }
}
