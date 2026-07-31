using Google.Protobuf.WellKnownTypes;
using Grpc.Core;
using NonProxy.Common.V1;
using NonProxy.Control.V1;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient
{
    private static readonly TimeSpan ExitProbeTimeout = TimeSpan.FromSeconds(10);
    private static readonly TimeSpan ExitProbeRpcTimeout = TimeSpan.FromSeconds(15);

    public Task<ListExitProbesResponse> ListExitProbesAsync(
        int pageSize,
        string pageToken,
        CancellationToken cancellationToken)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(pageSize, 1);
        ArgumentOutOfRangeException.ThrowIfGreaterThan(pageSize, 200);
        return ExecuteAsync(
            () => Client.ListExitProbesAsync(
                new ListExitProbesRequest
                {
                    Page = new PageRequest
                    {
                        PageSize = checked((uint)pageSize),
                        PageToken = pageToken ?? string.Empty,
                    },
                },
                ReadOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<VerifyExitResponse> VerifyExitAsync(
        string? outboundId,
        CancellationToken cancellationToken)
    {
        var context = await _contextProvider.CreateAsync(
            "verify-exit",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.VerifyExitAsync(
                CreateVerifyExitRequest(outboundId, context),
                Options(ExitProbeRpcTimeout, cancellationToken)).ResponseAsync);
    }

    internal static VerifyExitRequest CreateVerifyExitRequest(
        string? outboundId,
        OperationContext context)
    {
        ArgumentNullException.ThrowIfNull(context);
        if (outboundId is not null)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        }
        return new VerifyExitRequest
        {
            Context = context,
            Route = outboundId is null
                ? ExitProbeRouteKind.Direct
                : ExitProbeRouteKind.Proxy,
            OutboundId = outboundId is null
                ? string.Empty
                : outboundId,
            Timeout = Duration.FromTimeSpan(ExitProbeTimeout),
        };
    }
}
