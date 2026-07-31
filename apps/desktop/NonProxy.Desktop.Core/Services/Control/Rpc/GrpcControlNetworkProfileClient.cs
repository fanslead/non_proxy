using NonProxy.Control.V1;
using NonProxy.Policy.V1;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient
{
    public Task<ListNetworkProfilesResponse> ListNetworkProfilesAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        return ExecuteAsync(
            () => Client.ListNetworkProfilesAsync(
                new ListNetworkProfilesRequest { Page = FullPage(pageToken) },
                ReadOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<UpsertNetworkProfileResponse> UpsertNetworkProfileAsync(
        NetworkProfileSpec profile,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(profile);
        ArgumentOutOfRangeException.ThrowIfEqual(expectedRevision, ulong.MaxValue);
        var context = await _contextProvider.CreateAsync(
            "upsert-network-profile",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.UpsertNetworkProfileAsync(
                new UpsertNetworkProfileRequest
                {
                    Context = context,
                    Profile = profile,
                    ExpectedRevision = expectedRevision,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<DeleteNetworkProfileResponse> DeleteNetworkProfileAsync(
        string profileId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);
        ArgumentOutOfRangeException.ThrowIfZero(expectedRevision);
        var context = await _contextProvider.CreateAsync(
            "delete-network-profile",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.DeleteNetworkProfileAsync(
                new DeleteNetworkProfileRequest
                {
                    Context = context,
                    ProfileId = profileId,
                    ExpectedRevision = expectedRevision,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }
}
