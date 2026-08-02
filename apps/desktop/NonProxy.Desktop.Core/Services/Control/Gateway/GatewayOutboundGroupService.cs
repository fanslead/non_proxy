using System.Text;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Policy.V1;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayOutboundGroupService(
    IControlRpcClient client) : IOutboundGroupService
{
    private const int MaximumPages = 100;
    private const int MinimumMembers = 2;
    private const int MaximumMembers = 32;
    private const int MaximumTextBytes = 128;

    public async Task<OutboundGroupCatalog> ListAsync(
        CancellationToken cancellationToken)
    {
        var groups = new List<OutboundGroupListItem>();
        var tokens = new HashSet<string>(StringComparer.Ordinal);
        var pageToken = string.Empty;
        ulong? routingRevision = null;
        for (var page = 0; page < MaximumPages; page++)
        {
            if (!tokens.Add(pageToken))
            {
                throw InvalidPaging();
            }

            var response = await client.ListOutboundGroupsAsync(
                pageToken,
                cancellationToken);
            if (response.RoutingRevision == 0
                || routingRevision is not null
                    && routingRevision != response.RoutingRevision)
            {
                throw InvalidPaging();
            }

            routingRevision = response.RoutingRevision;
            groups.AddRange(response.Groups.Select(ToItem));
            pageToken = response.Page?.NextPageToken ?? throw InvalidPaging();
            if (string.IsNullOrEmpty(pageToken))
            {
                ValidateCatalog(groups);
                return new OutboundGroupCatalog(
                    groups,
                    routingRevision.Value,
                    groups.SingleOrDefault(group => group.IsDefault)?.Id);
            }
        }

        throw InvalidPaging();
    }

    public async Task<OutboundGroupMutation> SaveAsync(
        OutboundGroupDraft draft,
        CancellationToken cancellationToken)
    {
        var normalized = ValidateDraft(draft);
        var response = await client.UpsertOutboundGroupAsync(
            normalized.Id,
            normalized.Name,
            normalized.OutboundIds,
            normalized.ExpectedRevision ?? 0,
            cancellationToken);
        var result = response.Result
            ?? throw InvalidContract("控制服务没有返回线路组保存结果。");
        if (result.Error is { } error)
        {
            return RejectedMutation(error);
        }

        var group = result.Group is null
            ? throw InvalidContract("控制服务没有返回已保存的线路组。")
            : ToItem(result.Group);
        var expectedResultRevision = (normalized.ExpectedRevision ?? 0) + 1;
        if (!string.Equals(group.Id, normalized.Id, StringComparison.Ordinal)
            || group.Revision != expectedResultRevision
            || result.RoutingRevision == 0)
        {
            throw InvalidContract("控制服务返回了不匹配的线路组状态。");
        }

        ulong? pendingSnapshotVersion = null;
        if (group.IsDefault)
        {
            if (result.Snapshot is not { } snapshot
                || snapshot.SnapshotVersion == 0
                || snapshot.State != SnapshotState.PendingAck)
            {
                throw InvalidContract("默认线路组更新缺少待确认的路由快照。");
            }
            pendingSnapshotVersion = snapshot.SnapshotVersion;
        }
        else if (result.Snapshot is not null)
        {
            throw InvalidContract("非默认线路组不应触发路由快照。");
        }

        return new OutboundGroupMutation(
            true,
            pendingSnapshotVersion is null
                ? "NP_OUTBOUND_GROUP_SAVED"
                : "NP_SNAPSHOT_PENDING_ACK",
            pendingSnapshotVersion is null
                ? "自动切换线路组已保存。"
                : "自动切换线路组已保存，新的路由快照正在等待系统组件确认。",
            group,
            result.RoutingRevision,
            pendingSnapshotVersion);
    }

    public async Task<OutboundGroupDeletion> DeleteAsync(
        string groupId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ValidateExisting(groupId, expectedRevision);
        var response = await client.DeleteOutboundGroupAsync(
            groupId,
            expectedRevision,
            cancellationToken);
        if (!string.Equals(response.GroupId, groupId, StringComparison.Ordinal))
        {
            throw InvalidContract("控制服务返回了不匹配的线路组删除结果。");
        }
        if (response.Error is { } error)
        {
            return new OutboundGroupDeletion(
                false,
                error.Code,
                UserMessage(error.Code));
        }
        return new OutboundGroupDeletion(
            true,
            "NP_OUTBOUND_GROUP_DELETED",
            "自动切换线路组已删除，成员代理配置仍然保留。");
    }

    public async Task<ApplyResult> SetDefaultAsync(
        string groupId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(groupId);
        if (expectedRoutingRevision is 0 or ulong.MaxValue)
        {
            throw InvalidRequest("默认路由修订号无效，请刷新后重试。");
        }
        var response = await client.SetDefaultOutboundGroupAsync(
            groupId,
            expectedRoutingRevision,
            cancellationToken);
        return GatewayOutboundService.MapRouteChange(
            response,
            expectedRoutingRevision,
            "自动切换线路组已设为默认，新的路由快照正在等待系统组件确认。",
            UserMessage);
    }

    private static OutboundGroupDraft ValidateDraft(OutboundGroupDraft draft)
    {
        ArgumentNullException.ThrowIfNull(draft);
        var id = draft.Id.Trim();
        var name = draft.Name.Trim();
        ValidateIdentifier(id);
        if (name.Length == 0
            || Encoding.UTF8.GetByteCount(name) > MaximumTextBytes
            || name.Any(char.IsControl))
        {
            throw InvalidRequest("线路组名称不能为空，且不能超过 128 字节。");
        }
        if (draft.ExpectedRevision is 0 or ulong.MaxValue)
        {
            throw InvalidRequest("线路组修订号无效，请刷新后重试。");
        }
        ArgumentNullException.ThrowIfNull(draft.OutboundIds);
        var members = draft.OutboundIds
            .Select(member => member.Trim())
            .ToArray();
        if (members.Length is < MinimumMembers or > MaximumMembers
            || members.Any(string.IsNullOrWhiteSpace)
            || members.Distinct(StringComparer.Ordinal).Count() != members.Length)
        {
            throw InvalidRequest("请按优先级选择 2 到 32 条不重复的完整代理线路。");
        }
        foreach (var member in members)
        {
            ValidateIdentifier(member);
        }
        return draft with { Id = id, Name = name, OutboundIds = members };
    }

    private static OutboundGroupListItem ToItem(OutboundGroupSummary group)
    {
        if (group.Strategy != OutboundGroupStrategy.Failover
            || group.Revision == 0)
        {
            throw InvalidContract("控制服务返回了不支持的线路组策略或修订号。");
        }
        if (!IsValidIdentifier(group.Id))
        {
            throw InvalidContract("控制服务返回了无效的线路组标识。");
        }
        if (string.IsNullOrWhiteSpace(group.DisplayName)
            || group.DisplayName != group.DisplayName.Trim()
            || Encoding.UTF8.GetByteCount(group.DisplayName) > MaximumTextBytes
            || group.DisplayName.Any(char.IsControl)
            || group.OutboundIds.Count is < MinimumMembers or > MaximumMembers
            || group.OutboundIds.Distinct(StringComparer.Ordinal).Count()
                != group.OutboundIds.Count)
        {
            throw InvalidContract("控制服务返回了无效的线路组内容。");
        }
        foreach (var member in group.OutboundIds)
        {
            if (!IsValidIdentifier(member))
            {
                throw InvalidContract("控制服务返回了无效的线路组成员标识。");
            }
        }
        return new OutboundGroupListItem(
            group.Id,
            group.DisplayName,
            group.OutboundIds.ToArray(),
            group.Revision,
            group.IsDefault);
    }

    private static void ValidateCatalog(List<OutboundGroupListItem> groups)
    {
        if (groups.Select(group => group.Id).Distinct(
                StringComparer.Ordinal).Count() != groups.Count
            || groups.Count(group => group.IsDefault) > 1)
        {
            throw InvalidPaging();
        }
    }

    private static OutboundGroupMutation RejectedMutation(ErrorDetail error)
    {
        return new OutboundGroupMutation(
            false,
            error.Code,
            UserMessage(error.Code),
            null,
            0);
    }

    private static string UserMessage(string code)
    {
        return code switch
        {
            "NP_STORAGE_OUTBOUND_GROUP_REVISION_CONFLICT" =>
                "线路组已被其他操作修改，请刷新后重试。",
            "NP_STORAGE_OUTBOUND_GROUP_INVALID" =>
                "线路组无效，请选择 2 到 32 条不重复的完整代理线路。",
            "NP_STORAGE_OUTBOUND_GROUP_MEMBER_NOT_FOUND" =>
                "部分成员线路已不存在，请刷新后重新选择。",
            "NP_STORAGE_OUTBOUND_GROUP_MEMBER_UNSUPPORTED" =>
                "部分成员不支持内置自动切换，请改选由 NonProxy 数据面连接的代理。",
            "NP_STORAGE_OUTBOUND_GROUP_IN_USE" =>
                "该线路组正在被默认路由或规则使用，请先切换相关设置。",
            "NP_ROUTING_REVISION_CONFLICT" =>
                "默认路由已被其他操作修改，请刷新后重试。",
            "NP_DEFAULT_OUTBOUND_UNAVAILABLE" =>
                "线路组或成员已不存在、已停用或能力不足，请刷新列表。",
            "NP_DEFAULT_OUTBOUND_UNVERIFIED" =>
                "线路组中还没有经过连续健康确认的可用成员，请稍后测试后重试。",
            "NP_SNAPSHOT_ALREADY_PENDING" =>
                "已有路由快照等待系统组件确认，请稍后刷新再试。",
            "NP_POLICY_COMPILE_REJECTED" =>
                "当前线路组能力不足以承载所有未匹配流量，请检查成员线路。",
            _ => "控制服务没有接受本次线路组操作。",
        };
    }

    private static void ValidateExisting(string groupId, ulong revision)
    {
        ValidateIdentifier(groupId);
        if (revision is 0 or ulong.MaxValue)
        {
            throw InvalidRequest("线路组修订号无效，请刷新后重试。");
        }
    }

    private static void ValidateIdentifier(string value)
    {
        if (!IsValidIdentifier(value))
        {
            throw InvalidRequest("线路组或成员标识无效，请刷新后重试。");
        }
    }

    internal static bool IsValidIdentifier(string value)
    {
        return !string.IsNullOrWhiteSpace(value)
            && value == value.Trim()
            && Encoding.UTF8.GetByteCount(value) <= MaximumTextBytes
            && value.All(character => char.IsAsciiLetterOrDigit(character)
                || character is '.' or '_' or ':' or '-');
    }

    private static ControlServiceException InvalidPaging()
    {
        return InvalidContract("控制服务返回了无效线路组分页游标。");
    }

    private static ControlServiceException InvalidContract(string message)
    {
        return new ControlServiceException("NP_CONTROL_CONTRACT_INVALID", message);
    }

    private static ControlServiceException InvalidRequest(string message)
    {
        return new ControlServiceException("NP_REQUEST_INVALID", message);
    }
}
