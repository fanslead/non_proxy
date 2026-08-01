using System.Security.Cryptography;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using NonProxy.Control.V1;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient
{
    private static readonly TimeSpan MinimumSubscriptionInterval = TimeSpan.FromMinutes(15);
    private static readonly TimeSpan MaximumSubscriptionInterval = TimeSpan.FromDays(7);

    public async Task<UpsertSubscriptionSourceResponse> UpsertSubscriptionSourceAsync(
        string sourceId,
        string displayName,
        byte[] endpointUrl,
        bool enabled,
        TimeSpan refreshInterval,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(endpointUrl);
        try
        {
            ValidateSubscriptionIdentity(sourceId, displayName);
            ValidateSubscriptionInterval(refreshInterval);
            var context = await _contextProvider.CreateAsync(
                "upsert-subscription",
                cancellationToken);
            var request = new UpsertSubscriptionSourceRequest
            {
                Context = context,
                SourceId = sourceId,
                DisplayName = displayName,
                EndpointUrl = UnsafeByteOperations.UnsafeWrap(endpointUrl),
                Enabled = enabled,
                RefreshInterval = Duration.FromTimeSpan(refreshInterval),
                ExpectedRevision = expectedRevision,
            };
            return await ExecuteAsync(
                () => Client.UpsertSubscriptionSourceAsync(
                    request,
                    MutationOptions(cancellationToken)).ResponseAsync);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(endpointUrl);
        }
    }

    public async Task<RefreshSubscriptionSourceResponse> RefreshSubscriptionSourceAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ValidateExistingSubscription(sourceId, expectedRevision);
        var context = await _contextProvider.CreateAsync(
            "refresh-subscription",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.RefreshSubscriptionSourceAsync(
                new RefreshSubscriptionSourceRequest
                {
                    Context = context,
                    SourceId = sourceId,
                    ExpectedRevision = expectedRevision,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<DeleteSubscriptionSourceResponse> DeleteSubscriptionSourceAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ValidateExistingSubscription(sourceId, expectedRevision);
        var context = await _contextProvider.CreateAsync(
            "delete-subscription",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.DeleteSubscriptionSourceAsync(
                new DeleteSubscriptionSourceRequest
                {
                    Context = context,
                    SourceId = sourceId,
                    ExpectedRevision = expectedRevision,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    private static void ValidateSubscriptionIdentity(string sourceId, string displayName)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(sourceId);
        ArgumentException.ThrowIfNullOrWhiteSpace(displayName);
    }

    private static void ValidateExistingSubscription(string sourceId, ulong expectedRevision)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(sourceId);
        ArgumentOutOfRangeException.ThrowIfZero(expectedRevision);
    }

    private static void ValidateSubscriptionInterval(TimeSpan value)
    {
        if (value < MinimumSubscriptionInterval
            || value > MaximumSubscriptionInterval
            || value.Ticks % TimeSpan.TicksPerSecond != 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(value),
                "订阅刷新间隔必须是 15 分钟到 7 天之间的整秒数。");
        }
    }
}
