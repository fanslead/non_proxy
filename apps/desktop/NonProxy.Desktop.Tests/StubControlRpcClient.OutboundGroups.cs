using NonProxy.Control.V1;

namespace NonProxy.Desktop.Tests;

internal sealed partial class StubControlRpcClient
{
    public ListOutboundGroupsResponse OutboundGroupsResponse { get; set; } = new()
    {
        Page = new NonProxy.Common.V1.PageResponse(),
        RoutingRevision = 1,
    };

    public Queue<ListOutboundGroupsResponse> OutboundGroupsResponses { get; } = new();

    public UpsertOutboundGroupResponse UpsertOutboundGroupResponse { get; set; } = new();

    public DeleteOutboundGroupResponse DeleteOutboundGroupResponse { get; set; } = new();

    public string? LastOutboundGroupId { get; private set; }

    public string? LastOutboundGroupDisplayName { get; private set; }

    public IReadOnlyList<string>? LastOutboundGroupMembers { get; private set; }

    public string? LastDefaultOutboundGroupId { get; private set; }

    public Task<ListOutboundGroupsResponse> ListOutboundGroupsAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(OutboundGroupsResponses.Count > 0
            ? OutboundGroupsResponses.Dequeue()
            : OutboundGroupsResponse);
    }

    public Task<UpsertOutboundGroupResponse> UpsertOutboundGroupAsync(
        string groupId,
        string displayName,
        IReadOnlyList<string> outboundIds,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastOutboundGroupId = groupId;
        LastOutboundGroupDisplayName = displayName;
        LastOutboundGroupMembers = outboundIds.ToArray();
        LastExpectedRevision = expectedRevision;
        return Task.FromResult(UpsertOutboundGroupResponse);
    }

    public Task<DeleteOutboundGroupResponse> DeleteOutboundGroupAsync(
        string groupId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastOutboundGroupId = groupId;
        LastExpectedRevision = expectedRevision;
        return Task.FromResult(DeleteOutboundGroupResponse);
    }

    public Task<SetDefaultRouteResponse> SetDefaultOutboundGroupAsync(
        string groupId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        LastDefaultOutboundGroupId = groupId;
        LastExpectedRoutingRevision = expectedRoutingRevision;
        LastRouteWasDirect = false;
        return Task.FromResult(SetDefaultRouteResponse);
    }
}
