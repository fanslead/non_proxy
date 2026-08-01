using System.Text;
using NonProxy.Control.V1;

namespace NonProxy.Desktop.Tests;

internal sealed partial class StubControlRpcClient
{
    public ListSubscriptionSourcesResponse SubscriptionSourcesResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
    };

    public Queue<ListSubscriptionSourcesResponse> SubscriptionSourcesResponses { get; } = new();

    public UpsertSubscriptionSourceResponse UpsertSubscriptionResponse { get; set; } = new();

    public RefreshSubscriptionSourceResponse RefreshSubscriptionResponse { get; set; } = new();

    public DeleteSubscriptionSourceResponse DeleteSubscriptionResponse { get; set; } = new();

    public int ListSubscriptionSourcesCallCount { get; private set; }

    public string? LastSubscriptionId { get; private set; }

    public string? LastSubscriptionDisplayName { get; private set; }

    public string? LastSubscriptionEndpointText { get; private set; }

    public byte[]? LastSubscriptionEndpointBuffer { get; private set; }

    public bool LastSubscriptionEnabled { get; private set; }

    public TimeSpan LastSubscriptionRefreshInterval { get; private set; }

    public ulong LastSubscriptionExpectedRevision { get; private set; }

    public Task<ListSubscriptionSourcesResponse> ListSubscriptionSourcesAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        ListSubscriptionSourcesCallCount++;
        return Task.FromResult(SubscriptionSourcesResponses.Count > 0
            ? SubscriptionSourcesResponses.Dequeue()
            : SubscriptionSourcesResponse);
    }

    public Task<UpsertSubscriptionSourceResponse> UpsertSubscriptionSourceAsync(
        string sourceId,
        string displayName,
        byte[] endpointUrl,
        bool enabled,
        TimeSpan refreshInterval,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastSubscriptionId = sourceId;
        LastSubscriptionDisplayName = displayName;
        LastSubscriptionEndpointText = Encoding.UTF8.GetString(endpointUrl);
        LastSubscriptionEndpointBuffer = endpointUrl;
        LastSubscriptionEnabled = enabled;
        LastSubscriptionRefreshInterval = refreshInterval;
        LastSubscriptionExpectedRevision = expectedRevision;
        return Task.FromResult(UpsertSubscriptionResponse);
    }

    public Task<RefreshSubscriptionSourceResponse> RefreshSubscriptionSourceAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastSubscriptionId = sourceId;
        LastSubscriptionExpectedRevision = expectedRevision;
        return Task.FromResult(RefreshSubscriptionResponse);
    }

    public Task<DeleteSubscriptionSourceResponse> DeleteSubscriptionSourceAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastSubscriptionId = sourceId;
        LastSubscriptionExpectedRevision = expectedRevision;
        return Task.FromResult(DeleteSubscriptionResponse);
    }
}
