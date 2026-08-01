using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayPolicyService : IPolicyService
{
    private const int MaximumPages = 100;

    private readonly IControlRpcClient _client;
    private readonly PolicyContractMapper _mapper;

    public GatewayPolicyService(
        IControlRpcClient client,
        PolicyContractMapper mapper)
    {
        _client = client;
        _mapper = mapper;
    }

    public async Task<PolicyCatalog> GetCatalogAsync(
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
            "规则状态持续变化，请稍后刷新。");
    }

    public async Task<ApplyResult> SaveAsync(
        PolicyDraft draft,
        CancellationToken cancellationToken)
    {
        var contract = _mapper.ToContract(draft);
        var response = await _client.UpsertPolicyAsync(
            contract.Policy,
            contract.ExpectedRevision,
            cancellationToken);
        if (response.Result?.Error is not null || response.Result?.Policy is null)
        {
            return MutationResultMapper.Rejected(response.Result);
        }

        return await PublishSavedDraftAsync(
            "规则已保存",
            cancellationToken);
    }

    public async Task<ApplyResult> DeleteAsync(
        string policyId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(policyId);
        var catalog = await GetCatalogAsync(cancellationToken);
        var policy = catalog.Items.FirstOrDefault(item =>
            string.Equals(item.Id, policyId, StringComparison.Ordinal));
        if (policy is null)
        {
            return new ApplyResult(
                false,
                false,
                "NP_POLICY_NOT_FOUND",
                "规则已经不存在，请刷新列表。",
                null);
        }

        if (policy.State == PolicyApplyState.PendingRemoval)
        {
            return new ApplyResult(
                true,
                false,
                "NP_POLICY_PENDING_REMOVAL",
                "该规则已经处于待移除状态。",
                policy.SnapshotVersion);
        }

        var response = await _client.DeletePolicyAsync(
            policy.Id,
            policy.Revision,
            cancellationToken);
        if (response.Result?.Error is not null)
        {
            return MutationResultMapper.Rejected(response.Result);
        }

        return await PublishSavedDraftAsync(
            "规则删除草稿已保存",
            cancellationToken);
    }

    public async Task<ApplyResult> RollBackAsync(
        ulong snapshotVersion,
        CancellationToken cancellationToken)
    {
        var catalog = await GetCatalogAsync(cancellationToken);
        if (catalog.PendingSnapshotVersion is not null)
        {
            return new ApplyResult(
                false,
                false,
                "NP_SNAPSHOT_ALREADY_PENDING",
                "已有快照等待系统组件确认，请稍后刷新再恢复。",
                catalog.PendingSnapshotVersion);
        }
        if (catalog.ActiveSnapshotVersion is not { } activeVersion
            || catalog.PreviousEffectiveSnapshotVersion != snapshotVersion)
        {
            return new ApplyResult(
                false,
                false,
                "NP_SNAPSHOT_RESTORE_TARGET_CHANGED",
                "当前生效配置已经变化，请刷新后重新确认恢复目标。",
                catalog.ActiveSnapshotVersion);
        }

        var response = await _client.RollBackAsync(
            snapshotVersion,
            activeVersion,
            cancellationToken);
        if (response.Result?.Error is not null)
        {
            return MutationResultMapper.Rejected(response.Result);
        }

        return MutationResultMapper.Published(
            response.Result,
            "回滚请求已保存");
    }

    private async Task<ApplyResult> PublishSavedDraftAsync(
        string acceptedPrefix,
        CancellationToken cancellationToken)
    {
        try
        {
            var published = await _client.ApplySnapshotAsync(cancellationToken);
            return MutationResultMapper.AfterSavedDraft(
                published.Result,
                acceptedPrefix);
        }
        catch (ControlServiceException exception)
        {
            return new ApplyResult(
                true,
                false,
                "NP_POLICY_PUBLISH_UNKNOWN",
                $"{acceptedPrefix}，但无法确认发布状态：{exception.UserMessage}",
                null);
        }
    }

    private async Task<PolicyCatalog> LoadCatalogOnceAsync(
        CancellationToken cancellationToken)
    {
        var items = new List<PolicyListItem>();
        var tokens = new HashSet<string>(StringComparer.Ordinal);
        var pageToken = string.Empty;
        ulong? activeVersion = null;
        ulong? pendingVersion = null;
        ulong? previousEffectiveVersion = null;
        ulong? catalogGeneration = null;
        for (var page = 0; page < MaximumPages; page++)
        {
            if (!tokens.Add(pageToken))
            {
                throw InvalidPaging();
            }

            var response = await _client.ListPoliciesAsync(
                pageToken,
                cancellationToken);
            var responseActive = OptionalVersion(response.ActiveSnapshotVersion);
            var responsePending = OptionalVersion(response.PendingSnapshotVersion);
            var responsePrevious = OptionalVersion(
                response.PreviousEffectiveSnapshotVersion);
            if (page > 0
                && (activeVersion != responseActive
                    || pendingVersion != responsePending
                    || previousEffectiveVersion != responsePrevious
                    || catalogGeneration != response.PolicyCatalogGeneration))
            {
                throw new CatalogChangedException();
            }

            activeVersion = responseActive;
            pendingVersion = responsePending;
            previousEffectiveVersion = responsePrevious;
            catalogGeneration = response.PolicyCatalogGeneration;
            if (response.PolicyStatuses.Count > 0)
            {
                items.AddRange(response.PolicyStatuses.Select(
                    PolicyContractMapper.FromStatus));
            }
            else
            {
                items.AddRange(response.Policies.Select(
                    PolicyContractMapper.FromLegacy));
            }

            pageToken = response.Page?.NextPageToken ?? string.Empty;
            if (string.IsNullOrEmpty(pageToken))
            {
                if (previousEffectiveVersion is { } previous
                    && (activeVersion is not { } active || previous >= active))
                {
                    throw InvalidPaging();
                }
                if (items.Select(item => item.Id).Distinct(
                        StringComparer.Ordinal).Count() != items.Count)
                {
                    throw InvalidPaging();
                }

                return new PolicyCatalog(
                    items,
                    activeVersion,
                    DateTimeOffset.UtcNow,
                    pendingVersion,
                    previousEffectiveVersion);
            }
        }

        throw InvalidPaging();
    }

    private static ulong? OptionalVersion(ulong value)
    {
        return value == 0 ? null : value;
    }

    private static ControlServiceException InvalidPaging()
    {
        return new ControlServiceException(
            "NP_CONTROL_PAGING_INVALID",
            "控制服务返回了无效分页游标。");
    }

    private sealed class CatalogChangedException : Exception;
}
