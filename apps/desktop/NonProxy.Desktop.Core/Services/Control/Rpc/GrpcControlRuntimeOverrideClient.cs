using Google.Protobuf.WellKnownTypes;
using NonProxy.Control.V1;
using NonProxy.Policy.V1;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient
{
    public Task<GetRuntimeOverrideStatusResponse> GetRuntimeOverrideStatusAsync(
        CancellationToken cancellationToken)
    {
        return ExecuteAsync(
            () => Client.GetRuntimeOverrideStatusAsync(
                new GetRuntimeOverrideStatusRequest(),
                ReadOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<SetRuntimeOverrideResponse> SetRuntimeOverrideAsync(
        RuntimeOverrideMode mode,
        TimeSpan duration,
        string? outboundId,
        ulong expectedActiveSnapshotVersion,
        CancellationToken cancellationToken)
    {
        var context = await _contextProvider.CreateAsync(
            "set-runtime-override",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.SetRuntimeOverrideAsync(
                CreateSetRuntimeOverrideRequest(
                    context,
                    mode,
                    duration,
                    outboundId,
                    expectedActiveSnapshotVersion),
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<ClearRuntimeOverrideResponse> ClearRuntimeOverrideAsync(
        ulong expectedActiveSnapshotVersion,
        CancellationToken cancellationToken)
    {
        ArgumentOutOfRangeException.ThrowIfZero(expectedActiveSnapshotVersion);
        var context = await _contextProvider.CreateAsync(
            "clear-runtime-override",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.ClearRuntimeOverrideAsync(
                new ClearRuntimeOverrideRequest
                {
                    Context = context,
                    ExpectedActiveSnapshotVersion = expectedActiveSnapshotVersion,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    internal static SetRuntimeOverrideRequest CreateSetRuntimeOverrideRequest(
        OperationContext context,
        RuntimeOverrideMode mode,
        TimeSpan duration,
        string? outboundId,
        ulong expectedActiveSnapshotVersion)
    {
        ArgumentNullException.ThrowIfNull(context);
        ArgumentOutOfRangeException.ThrowIfZero(expectedActiveSnapshotVersion);
        if (duration < TimeSpan.FromSeconds(1)
            || duration > TimeSpan.FromHours(1)
            || duration.Ticks % TimeSpan.TicksPerMillisecond != 0)
        {
            throw new ArgumentOutOfRangeException(nameof(duration));
        }
        if (mode == RuntimeOverrideMode.Proxy)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        }
        else if (!string.IsNullOrEmpty(outboundId))
        {
            throw new ArgumentException(
                "非代理运行态覆盖不能指定出口。",
                nameof(outboundId));
        }
        if (mode is not RuntimeOverrideMode.Paused
            and not RuntimeOverrideMode.Direct
            and not RuntimeOverrideMode.Proxy)
        {
            throw new ArgumentOutOfRangeException(nameof(mode));
        }

        return new SetRuntimeOverrideRequest
        {
            Context = context,
            Mode = mode,
            Duration = Duration.FromTimeSpan(duration),
            OutboundId = outboundId ?? string.Empty,
            ExpectedActiveSnapshotVersion = expectedActiveSnapshotVersion,
        };
    }
}
