using NonProxy.Common.V1;
using NonProxy.Control.V1;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient
{
    public Task<GetSystemStatusResponse> GetSystemStatusAsync(
        CancellationToken cancellationToken)
    {
        return ExecuteAsync(
            () => Client.GetSystemStatusAsync(
                new GetSystemStatusRequest(),
                ReadOptions(cancellationToken)).ResponseAsync);
    }

    public Task<ListPoliciesResponse> ListPoliciesAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        return ExecuteAsync(
            () => Client.ListPoliciesAsync(
                new ListPoliciesRequest
                {
                    IncludeDisabled = true,
                    Page = FullPage(pageToken),
                },
                ReadOptions(cancellationToken)).ResponseAsync);
    }

    public Task<ListOutboundsResponse> ListOutboundsAsync(
        string pageToken,
        CancellationToken cancellationToken)
    {
        return ExecuteAsync(
            () => Client.ListOutboundsAsync(
                new ListOutboundsRequest { Page = FullPage(pageToken) },
                ReadOptions(cancellationToken)).ResponseAsync);
    }

    public Task<ListConnectionDecisionsResponse> ListConnectionDecisionsAsync(
        int pageSize,
        string pageToken,
        CancellationToken cancellationToken)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(pageSize, 1);
        ArgumentOutOfRangeException.ThrowIfGreaterThan(pageSize, 200);
        return ExecuteAsync(
            () => Client.ListConnectionDecisionsAsync(
                new ListConnectionDecisionsRequest
                {
                    Page = new PageRequest
                    {
                        PageSize = checked((uint)pageSize),
                        PageToken = pageToken ?? string.Empty,
                    },
                },
                ReadOptions(cancellationToken)).ResponseAsync);
    }

    private static PageRequest FullPage(string pageToken)
    {
        return new PageRequest
        {
            PageSize = 200,
            PageToken = pageToken ?? string.Empty,
        };
    }
}
