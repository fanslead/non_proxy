using Google.Protobuf.WellKnownTypes;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;
using NonProxy.Policy.V1;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayRuntimeOverrideServiceTests
{
    [Fact]
    public async Task SetUsesCurrentActiveVersionAndKeepsPendingTruthful()
    {
        var rpc = new StubControlRpcClient
        {
            RuntimeOverrideStatusResponse = new GetRuntimeOverrideStatusResponse
            {
                ActiveSnapshotVersion = 7,
            },
            SetRuntimeOverrideResponse = new SetRuntimeOverrideResponse
            {
                Result = PendingResult(8),
            },
        };
        var service = new GatewayRuntimeOverrideService(rpc);

        var result = await service.SetAsync(
            RuntimeOverrideKind.Paused,
            null,
            TimeSpan.FromMinutes(5),
            TestContext.Current.CancellationToken);

        Assert.True(result.Accepted);
        Assert.False(result.Applied);
        Assert.Equal(8UL, result.SnapshotVersion);
        Assert.Equal(RuntimeOverrideMode.Paused, rpc.LastRuntimeOverrideMode);
        Assert.Equal(TimeSpan.FromMinutes(5), rpc.LastRuntimeOverrideDuration);
        Assert.Equal(7UL, rpc.LastExpectedActiveSnapshotVersion);
    }

    [Fact]
    public async Task StatusDistinguishesActivePendingAndPendingClear()
    {
        var rpc = new StubControlRpcClient
        {
            RuntimeOverrideStatusResponse = new GetRuntimeOverrideStatusResponse
            {
                ActiveSnapshotVersion = 4,
                PendingSnapshotVersion = 5,
                ActiveOverride = Override(RuntimeOverrideMode.Direct, 4_102_444_800),
                PendingClearsOverride = true,
            },
        };
        var service = new GatewayRuntimeOverrideService(rpc);

        var status = await service.GetStatusAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal(RuntimeOverrideKind.Direct, status.Active?.Kind);
        Assert.Null(status.Pending);
        Assert.True(status.PendingClearsOverride);
        Assert.True(status.HasPendingMutation);
        Assert.False(status.CanRequest);
    }

    [Fact]
    public async Task PendingSnapshotPreventsASecondMutationBeforeRpcCall()
    {
        var rpc = new StubControlRpcClient
        {
            RuntimeOverrideStatusResponse = new GetRuntimeOverrideStatusResponse
            {
                ActiveSnapshotVersion = 4,
                PendingSnapshotVersion = 5,
                PendingOverride = Override(RuntimeOverrideMode.Paused, 4_102_444_800),
            },
        };
        var service = new GatewayRuntimeOverrideService(rpc);

        var exception = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.SetAsync(
                RuntimeOverrideKind.Direct,
                null,
                TimeSpan.FromMinutes(5),
                TestContext.Current.CancellationToken));

        Assert.Equal("NP_SNAPSHOT_ALREADY_PENDING", exception.Code);
        Assert.Equal(RuntimeOverrideMode.Unspecified, rpc.LastRuntimeOverrideMode);
    }

    [Fact]
    public async Task ActiveOverrideWithoutActiveSnapshotVersionIsRejected()
    {
        var rpc = new StubControlRpcClient
        {
            RuntimeOverrideStatusResponse = new GetRuntimeOverrideStatusResponse
            {
                ActiveOverride = Override(RuntimeOverrideMode.Direct, 4_102_444_800),
            },
        };
        var service = new GatewayRuntimeOverrideService(rpc);

        var exception = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.GetStatusAsync(TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", exception.Code);
    }

    private static RuntimeRoutingOverride Override(
        RuntimeOverrideMode mode,
        long expiresAtSeconds)
    {
        return new RuntimeRoutingOverride
        {
            Mode = mode,
            ExpiresAt = Timestamp.FromDateTimeOffset(
                DateTimeOffset.FromUnixTimeSeconds(expiresAtSeconds)),
        };
    }

    private static PolicyMutationResult PendingResult(ulong version)
    {
        return new PolicyMutationResult
        {
            Snapshot = new PolicySnapshotMetadata
            {
                SnapshotVersion = version,
                State = SnapshotState.PendingAck,
            },
        };
    }
}
