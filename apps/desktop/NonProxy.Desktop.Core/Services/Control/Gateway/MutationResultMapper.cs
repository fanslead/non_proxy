using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Policy.V1;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public static class MutationResultMapper
{
    public static ApplyResult Rejected(PolicyMutationResult? result)
    {
        if (result?.Error is not { } error)
        {
            throw new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "控制服务没有返回规则操作结果。");
        }

        return new ApplyResult(
            false,
            false,
            error.Code,
            UserMessage(error, result.Conflicts),
            null);
    }

    public static ApplyResult AfterSavedDraft(
        PolicyMutationResult? result,
        string acceptedPrefix)
    {
        if (result?.Error is { } error)
        {
            return new ApplyResult(
                true,
                false,
                error.Code,
                $"{acceptedPrefix}，但尚未发布：{UserMessage(error, result.Conflicts)}",
                null);
        }

        return Published(result, acceptedPrefix);
    }

    public static ApplyResult Published(
        PolicyMutationResult? result,
        string acceptedPrefix)
    {
        if (result?.Error is not null)
        {
            return Rejected(result);
        }

        var snapshot = result?.Snapshot
            ?? throw new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "控制服务没有返回快照发布结果。");
        var applied = snapshot.State == SnapshotState.Active;
        var message = applied
            ? $"{acceptedPrefix}，快照 v{snapshot.SnapshotVersion} 已生效。"
            : $"{acceptedPrefix}，快照 v{snapshot.SnapshotVersion} 正等待系统组件确认。";
        return new ApplyResult(
            true,
            applied,
            applied ? "NP_POLICY_APPLIED" : "NP_POLICY_PENDING",
            message,
            snapshot.SnapshotVersion);
    }

    private static string UserMessage(
        ErrorDetail error,
        IEnumerable<PolicyConflict> conflicts)
    {
        var conflict = conflicts.FirstOrDefault();
        if (conflict is not null && !string.IsNullOrWhiteSpace(conflict.Message))
        {
            return conflict.Message;
        }

        return error.Code switch
        {
            "NP_POLICY_REVISION_CONFLICT" => "规则已被其他操作修改，请刷新后重试。",
            "NP_SNAPSHOT_ALREADY_PENDING" => "已有快照等待系统组件确认，请稍后再发布。",
            "NP_SNAPSHOT_ACTIVE_VERSION_CONFLICT" => "当前生效配置已经变化，请刷新后重新确认本次操作。",
            "NP_POLICY_COMPILE_REJECTED" => "规则之间存在冲突，请检查后重试。",
            "NP_RUNTIME_OVERRIDE_NOT_ACTIVE" => "当前没有需要取消的限时运行模式。",
            "NP_RUNTIME_OVERRIDE_DURATION_INVALID" => "限时运行时长必须在 1 秒到 1 小时之间。",
            "NP_RUNTIME_OVERRIDE_ACTIVE_SNAPSHOT_MISSING" => "当前没有活动快照，无法切换限时运行模式。",
            "NP_REQUEST_INVALID" => "请求内容无效，请检查输入。",
            "NP_STORAGE_FAILURE" => "规则存储暂时不可用。",
            _ => "控制服务没有接受本次操作。",
        };
    }
}
