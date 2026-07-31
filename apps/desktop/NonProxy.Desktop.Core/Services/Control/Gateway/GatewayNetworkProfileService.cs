using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayNetworkProfileService(
    IControlRpcClient client) : INetworkProfileService
{
    private const int MaximumPages = 100;

    public async Task<NetworkProfileCatalog> GetCatalogAsync(
        CancellationToken cancellationToken)
    {
        for (var attempt = 0; attempt < 2; attempt++)
        {
            try
            {
                return await LoadCatalogOnceAsync(cancellationToken);
            }
            catch (CatalogChangedException) when (attempt == 0)
            {
            }
        }

        throw new ControlServiceException(
            "NP_CONTROL_STATE_CHANGED",
            "网络配置持续变化，请稍后刷新。");
    }

    public async Task<NetworkProfileMutation> SaveAsync(
        NetworkProfileDraft draft,
        CancellationToken cancellationToken)
    {
        var mapped = NetworkProfileContractMapper.ToContract(draft);
        var response = await client.UpsertNetworkProfileAsync(
            mapped.Profile,
            mapped.ExpectedRevision,
            cancellationToken);
        if (response.Result?.Error is { } error)
        {
            return Rejected(error);
        }

        var profile = response.Result?.Profile
            ?? throw InvalidContract("控制服务没有返回已保存的网络配置档。");
        return new NetworkProfileMutation(
            true,
            "NP_NETWORK_PROFILE_SAVED",
            "网络配置已保存；尚未创建或发布直连规则。",
            NetworkProfileContractMapper.FromContract(profile));
    }

    public async Task<NetworkProfileMutation> DeleteAsync(
        string profileId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);
        ArgumentOutOfRangeException.ThrowIfZero(expectedRevision);
        var response = await client.DeleteNetworkProfileAsync(
            profileId,
            expectedRevision,
            cancellationToken);
        if (response.Result?.Error is { } error)
        {
            return Rejected(error);
        }

        if (response.Result is null)
        {
            throw InvalidContract("控制服务没有返回网络配置删除结果。");
        }

        return new NetworkProfileMutation(
            true,
            "NP_NETWORK_PROFILE_DELETED",
            "网络配置已删除。",
            null);
    }

    private async Task<NetworkProfileCatalog> LoadCatalogOnceAsync(
        CancellationToken cancellationToken)
    {
        var items = new List<NetworkProfileListItem>();
        var tokens = new HashSet<string>(StringComparer.Ordinal);
        var pageToken = string.Empty;
        ulong? generation = null;
        for (var page = 0; page < MaximumPages; page++)
        {
            if (!tokens.Add(pageToken))
            {
                throw InvalidPaging();
            }

            var response = await client.ListNetworkProfilesAsync(
                pageToken,
                cancellationToken);
            if (generation is not null && generation != response.CatalogGeneration)
            {
                throw new CatalogChangedException();
            }

            generation = response.CatalogGeneration;
            items.AddRange(response.Profiles.Select(
                NetworkProfileContractMapper.FromContract));
            pageToken = response.Page?.NextPageToken ?? string.Empty;
            if (string.IsNullOrEmpty(pageToken))
            {
                var hasDuplicateId = items.Select(item => item.Id).Distinct(
                    StringComparer.Ordinal).Count() != items.Count;
                var hasDuplicateFingerprint = items.Select(item => (
                    item.FingerprintKind,
                    item.FingerprintValue)).Distinct().Count() != items.Count;
                if (hasDuplicateId || hasDuplicateFingerprint)
                {
                    throw InvalidPaging();
                }

                return new NetworkProfileCatalog(
                    items,
                    generation ?? 0,
                    DateTimeOffset.UtcNow);
            }
        }

        throw InvalidPaging();
    }

    private static NetworkProfileMutation Rejected(ErrorDetail error)
    {
        var message = error.Code switch
        {
            "NP_STORAGE_NETWORK_PROFILE_REVISION_CONFLICT" =>
                "网络配置已被其他操作修改，请刷新后重试。",
            "NP_STORAGE_NETWORK_PROFILE_FINGERPRINT_CONFLICT" =>
                "当前网络已经存在配置，请刷新列表。",
            "NP_STORAGE_NETWORK_PROFILE_IN_USE" =>
                "网络配置仍被规则引用，请先删除对应规则。",
            "NP_NETWORK_PROFILE_INVALID" =>
                "网络名称或脱敏指纹无效，请重新检测。",
            _ => "控制服务没有接受本次网络配置操作。",
        };
        return new NetworkProfileMutation(false, error.Code, message, null);
    }

    private static ControlServiceException InvalidPaging()
    {
        return InvalidContract("控制服务返回了无效网络配置分页游标。");
    }

    private static ControlServiceException InvalidContract(string message)
    {
        return new ControlServiceException("NP_CONTROL_CONTRACT_INVALID", message);
    }

    private sealed class CatalogChangedException : Exception;
}
