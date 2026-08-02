using NonProxy.Control.V1;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient
{
    public async Task<UpsertOutboundGroupResponse> UpsertOutboundGroupAsync(
        string groupId,
        string displayName,
        IReadOnlyList<string> outboundIds,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        var context = await _contextProvider.CreateAsync(
            "upsert-outbound-group",
            cancellationToken);
        var request = CreateUpsertOutboundGroupRequest(
            groupId,
            displayName,
            outboundIds,
            expectedRevision,
            context);
        return await ExecuteAsync(
            () => Client.UpsertOutboundGroupAsync(
                request,
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<DeleteOutboundGroupResponse> DeleteOutboundGroupAsync(
        string groupId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(groupId);
        ValidateGroupRevision(expectedRevision);
        var context = await _contextProvider.CreateAsync(
            "delete-outbound-group",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.DeleteOutboundGroupAsync(
                new DeleteOutboundGroupRequest
                {
                    Context = context,
                    GroupId = groupId,
                    ExpectedRevision = expectedRevision,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<SetDefaultRouteResponse> SetDefaultOutboundGroupAsync(
        string groupId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(groupId);
        ValidateRoutingRevision(expectedRoutingRevision);
        var context = await _contextProvider.CreateAsync(
            "set-default-outbound-group",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.SetDefaultRouteAsync(
                CreateSetDefaultOutboundGroupRequest(
                    groupId,
                    expectedRoutingRevision,
                    context),
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    internal static UpsertOutboundGroupRequest CreateUpsertOutboundGroupRequest(
        string groupId,
        string displayName,
        IReadOnlyList<string> outboundIds,
        ulong expectedRevision,
        OperationContext context)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(groupId);
        ArgumentException.ThrowIfNullOrWhiteSpace(displayName);
        ArgumentNullException.ThrowIfNull(outboundIds);
        ArgumentNullException.ThrowIfNull(context);
        ArgumentOutOfRangeException.ThrowIfEqual(
            expectedRevision,
            ulong.MaxValue);

        var request = new UpsertOutboundGroupRequest
        {
            Context = context,
            GroupId = groupId,
            DisplayName = displayName,
            Strategy = OutboundGroupStrategy.Failover,
            ExpectedRevision = expectedRevision,
        };
        request.OutboundIds.Add(outboundIds);
        return request;
    }

    internal static SetDefaultRouteRequest CreateSetDefaultOutboundGroupRequest(
        string groupId,
        ulong expectedRoutingRevision,
        OperationContext context)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(groupId);
        ValidateRoutingRevision(expectedRoutingRevision);
        ArgumentNullException.ThrowIfNull(context);
        return new SetDefaultRouteRequest
        {
            Context = context,
            OutboundGroupId = groupId,
            ExpectedRoutingRevision = expectedRoutingRevision,
        };
    }

    private static void ValidateGroupRevision(ulong revision)
    {
        ArgumentOutOfRangeException.ThrowIfZero(revision);
        ArgumentOutOfRangeException.ThrowIfEqual(revision, ulong.MaxValue);
    }
}
