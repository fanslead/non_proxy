using System.Security.Cryptography;
using System.Text;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewaySubscriptionService(
    IControlRpcClient client) : ISubscriptionService
{
    private const int MaximumPages = 100;
    private const int MaximumEndpointBytes = 4 * 1024;

    public async Task<SubscriptionCatalog> ListAsync(
        CancellationToken cancellationToken)
    {
        var items = new List<SubscriptionListItem>();
        var tokens = new HashSet<string>(StringComparer.Ordinal);
        var pageToken = string.Empty;
        for (var page = 0; page < MaximumPages; page++)
        {
            if (!tokens.Add(pageToken))
            {
                throw InvalidPaging();
            }

            var response = await client.ListSubscriptionSourcesAsync(
                pageToken,
                cancellationToken);
            items.AddRange(response.Sources.Select(SubscriptionContractMapper.ToItem));
            pageToken = response.Page?.NextPageToken
                ?? throw InvalidPaging();
            if (string.IsNullOrEmpty(pageToken))
            {
                if (items.Select(item => item.Id).Distinct(
                        StringComparer.Ordinal).Count() != items.Count)
                {
                    throw InvalidPaging();
                }

                return new SubscriptionCatalog(items, DateTimeOffset.UtcNow);
            }
        }

        throw InvalidPaging();
    }

    public async Task<SubscriptionMutation> SaveAsync(
        SubscriptionDraft draft,
        CancellationToken cancellationToken)
    {
        var normalized = ValidateDraft(draft);
        var endpoint = normalized.EndpointUrl is null
            ? Array.Empty<byte>()
            : Encoding.UTF8.GetBytes(normalized.EndpointUrl);
        UpsertSubscriptionSourceResponse response;
        try
        {
            response = await client.UpsertSubscriptionSourceAsync(
                normalized.Id,
                normalized.DisplayName,
                endpoint,
                normalized.Enabled,
                normalized.RefreshInterval,
                normalized.ExpectedRevision ?? 0,
                cancellationToken);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(endpoint);
        }

        var result = response.Result
            ?? throw InvalidContract("控制服务没有返回订阅保存结果。");
        return MapMutation(
            result,
            normalized.Id,
            normalized.ExpectedRevision,
            normalized.EndpointUrl is null
                ? "订阅设置已保存；地址仍安全保存在系统凭据库中。"
                : "订阅已保存并完成一次安全刷新。",
            normalized.EndpointUrl is null
                ? "NP_SUBSCRIPTION_SETTINGS_SAVED"
                : "NP_SUBSCRIPTION_SAVED");
    }

    public async Task<SubscriptionMutation> RefreshAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ValidateExisting(sourceId, expectedRevision);
        var response = await client.RefreshSubscriptionSourceAsync(
            sourceId,
            expectedRevision,
            cancellationToken);
        var result = response.Result
            ?? throw InvalidContract("控制服务没有返回订阅刷新结果。");
        return MapMutation(
            result,
            sourceId,
            expectedRevision,
            result.ContentUnchanged ? "订阅内容没有变化，现有节点保持不变。" : "订阅已刷新，节点列表已更新。",
            result.ContentUnchanged
                ? "NP_SUBSCRIPTION_UNCHANGED"
                : "NP_SUBSCRIPTION_REFRESHED",
            revisionMustIncrement: false);
    }

    public async Task<SubscriptionDeletion> DeleteAsync(
        string sourceId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ValidateExisting(sourceId, expectedRevision);
        var response = await client.DeleteSubscriptionSourceAsync(
            sourceId,
            expectedRevision,
            cancellationToken);
        if (response.Error is { } error)
        {
            return new SubscriptionDeletion(
                false,
                error.Code,
                UserMessage(error.Code),
                null,
                0,
                response.Warnings.ToArray());
        }
        if (!string.Equals(response.SourceId, sourceId, StringComparison.Ordinal))
        {
            throw InvalidContract("控制服务返回了不匹配的订阅删除结果。");
        }

        return new SubscriptionDeletion(
            true,
            "NP_SUBSCRIPTION_DELETED",
            $"订阅及其 {response.RemovedOutboundCount} 个未被引用的节点已删除。",
            response.SourceId,
            response.RemovedOutboundCount,
            response.Warnings.ToArray());
    }

    private static SubscriptionMutation MapMutation(
        SubscriptionMutationResult result,
        string sourceId,
        ulong? expectedRevision,
        string successMessage,
        string successCode,
        bool revisionMustIncrement = true)
    {
        if (result.Error is { } error)
        {
            return new SubscriptionMutation(
                false,
                error.Code,
                UserMessage(error.Code),
                null,
                false,
                result.Warnings.ToArray());
        }

        var item = result.Source is null
            ? throw InvalidContract("控制服务没有返回更新后的订阅状态。")
            : SubscriptionContractMapper.ToItem(result.Source);
        var expectedResultRevision = revisionMustIncrement
            ? (expectedRevision ?? 0) + 1
            : expectedRevision;
        if (!string.Equals(item.Id, sourceId, StringComparison.Ordinal)
            || item.Revision != expectedResultRevision)
        {
            throw InvalidContract("控制服务返回了不匹配的订阅状态。");
        }

        return new SubscriptionMutation(
            true,
            successCode,
            successMessage,
            item,
            result.ContentUnchanged,
            result.Warnings.ToArray());
    }

    private static SubscriptionDraft ValidateDraft(SubscriptionDraft draft)
    {
        ArgumentNullException.ThrowIfNull(draft);
        var id = draft.Id.Trim();
        var displayName = draft.DisplayName.Trim();
        SubscriptionContractMapper.ValidateIdentifier(id);
        SubscriptionContractMapper.ValidateDisplayName(displayName);
        SubscriptionContractMapper.ValidateInterval(draft.RefreshInterval);
        if (draft.ExpectedRevision is 0 or ulong.MaxValue)
        {
            throw InvalidRequest("编辑订阅时缺少有效修订号，请刷新后重试。");
        }

        var endpoint = string.IsNullOrWhiteSpace(draft.EndpointUrl)
            ? null
            : draft.EndpointUrl.Trim();
        if (draft.ExpectedRevision is null && endpoint is null)
        {
            throw InvalidRequest("添加订阅时必须填写 HTTPS 订阅地址。");
        }
        if (endpoint is not null)
        {
            ValidateEndpoint(endpoint);
        }

        return draft with { Id = id, DisplayName = displayName, EndpointUrl = endpoint };
    }

    private static void ValidateEndpoint(string endpoint)
    {
        if (Encoding.UTF8.GetByteCount(endpoint) > MaximumEndpointBytes
            || !Uri.TryCreate(endpoint, UriKind.Absolute, out var uri)
            || uri.Scheme != Uri.UriSchemeHttps
            || string.IsNullOrEmpty(uri.Host)
            || !string.IsNullOrEmpty(uri.UserInfo)
            || !string.IsNullOrEmpty(uri.Fragment))
        {
            throw InvalidRequest("订阅地址必须是有效的 HTTPS 地址，且不能包含账号信息或片段。");
        }
    }

    private static void ValidateExisting(string sourceId, ulong expectedRevision)
    {
        SubscriptionContractMapper.ValidateIdentifier(sourceId);
        if (expectedRevision is 0 or ulong.MaxValue)
        {
            throw InvalidRequest("订阅修订号无效，请刷新后重试。");
        }
    }

    private static string UserMessage(string code)
    {
        return code switch
        {
            "NP_STORAGE_SUBSCRIPTION_REVISION_CONFLICT" => "订阅已被其他操作修改，请刷新后重试。",
            "NP_STORAGE_SUBSCRIPTION_DEFAULT_OUTBOUND_REMOVED" => "该订阅包含当前默认代理，请先切换默认出口。",
            "NP_STORAGE_SUBSCRIPTION_OUTBOUND_IN_USE" => "该订阅的节点仍被规则或出口组引用，请先解除引用。",
            "NP_SUBSCRIPTION_ENDPOINT_REQUIRED" => "添加订阅时必须填写 HTTPS 订阅地址。",
            "NP_SUBSCRIPTION_ENDPOINT_INVALID" => "订阅地址无效，请检查是否为完整的 HTTPS 地址。",
            "NP_SUBSCRIPTION_ADDRESS_NOT_PUBLIC" => "订阅地址解析到了本机或私有网络，已为安全起见拒绝访问。",
            "NP_SUBSCRIPTION_RESOLVE_FAILED" => "无法解析订阅服务器地址，请检查网络或域名。",
            "NP_SUBSCRIPTION_CONNECT_FAILED" or "NP_SUBSCRIPTION_TIMEOUT" => "连接订阅服务器失败，请检查网络后重试。",
            "NP_SUBSCRIPTION_TLS_FAILED" => "订阅服务器的 HTTPS 证书验证失败。",
            "NP_SUBSCRIPTION_HTTP_STATUS_INVALID" => "订阅服务器返回了失败状态，请确认订阅仍有效。",
            "NP_SUBSCRIPTION_RESPONSE_TOO_LARGE" => "订阅内容超过安全上限，无法导入。",
            "NP_SUBSCRIPTION_CONTENT_INVALID" => "订阅内容无效；当前远程订阅仅接受 Shadowsocks 节点。",
            "NP_SUBSCRIPTION_NODE_DUPLICATE" => "订阅中存在重复节点，请联系订阅提供方修正。",
            "NP_CREDENTIAL_STORE_FAILED" => "系统凭据库暂时不可用，订阅没有完整保存。",
            "NP_SUBSCRIPTION_NOT_FOUND" => "订阅已不存在，请刷新列表。",
            _ => "控制服务没有接受本次订阅操作。",
        };
    }

    private static ControlServiceException InvalidPaging()
    {
        return InvalidContract("控制服务返回了无效订阅分页游标。");
    }

    private static ControlServiceException InvalidRequest(string message)
    {
        return new ControlServiceException("NP_REQUEST_INVALID", message);
    }

    private static ControlServiceException InvalidContract(string message)
    {
        return new ControlServiceException("NP_CONTROL_CONTRACT_INVALID", message);
    }
}
