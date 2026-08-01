using Google.Protobuf.WellKnownTypes;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using ProtoMode = NonProxy.Policy.V1.RuntimeOverrideMode;
using ProtoOverride = NonProxy.Policy.V1.RuntimeRoutingOverride;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayRuntimeOverrideService : IRuntimeOverrideService
{
    private readonly IControlRpcClient _client;

    public GatewayRuntimeOverrideService(IControlRpcClient client)
    {
        _client = client;
    }

    public async Task<RuntimeOverrideStatus> GetStatusAsync(
        CancellationToken cancellationToken)
    {
        var response = await _client.GetRuntimeOverrideStatusAsync(cancellationToken);
        var active = Map(response.ActiveOverride);
        var pending = Map(response.PendingOverride);
        var activeVersion = OptionalVersion(response.ActiveSnapshotVersion);
        var pendingVersion = OptionalVersion(response.PendingSnapshotVersion);
        if (active is not null && activeVersion is null)
        {
            throw InvalidContract();
        }
        if (response.PendingClearsOverride
            && (active is null || pending is not null || pendingVersion is null))
        {
            throw InvalidContract();
        }
        if (pending is not null && pendingVersion is null)
        {
            throw InvalidContract();
        }
        return new RuntimeOverrideStatus(
            true,
            active,
            pending,
            activeVersion,
            pendingVersion,
            response.PendingClearsOverride);
    }

    public async Task<ApplyResult> SetAsync(
        RuntimeOverrideKind kind,
        string? outboundId,
        TimeSpan duration,
        CancellationToken cancellationToken)
    {
        var status = await GetStatusAsync(cancellationToken);
        var activeVersion = RequireMutable(status);
        var response = await _client.SetRuntimeOverrideAsync(
            ToProto(kind),
            duration,
            outboundId,
            activeVersion,
            cancellationToken);
        return MutationResultMapper.Published(
            response.Result,
            "限时运行模式已提交");
    }

    public async Task<ApplyResult> ClearAsync(CancellationToken cancellationToken)
    {
        var status = await GetStatusAsync(cancellationToken);
        if (status.Active is null)
        {
            throw new ControlServiceException(
                "NP_RUNTIME_OVERRIDE_NOT_ACTIVE",
                "当前没有需要取消的限时运行模式。");
        }
        var response = await _client.ClearRuntimeOverrideAsync(
            RequireMutable(status),
            cancellationToken);
        return MutationResultMapper.Published(
            response.Result,
            "恢复常规策略的请求已提交");
    }

    private static RuntimeOverrideInfo? Map(ProtoOverride? value)
    {
        if (value is null)
        {
            return null;
        }
        if (value.ExpiresAt is null)
        {
            throw InvalidContract();
        }
        DateTimeOffset expiresAt;
        try
        {
            expiresAt = value.ExpiresAt.ToDateTimeOffset();
        }
        catch (InvalidOperationException)
        {
            throw InvalidContract();
        }
        var kind = value.Mode switch
        {
            ProtoMode.Paused when string.IsNullOrEmpty(value.OutboundId) =>
                RuntimeOverrideKind.Paused,
            ProtoMode.Direct when string.IsNullOrEmpty(value.OutboundId) =>
                RuntimeOverrideKind.Direct,
            ProtoMode.Proxy when !string.IsNullOrWhiteSpace(value.OutboundId) =>
                RuntimeOverrideKind.Proxy,
            _ => throw InvalidContract(),
        };
        return new RuntimeOverrideInfo(
            kind,
            string.IsNullOrEmpty(value.OutboundId) ? null : value.OutboundId,
            expiresAt);
    }

    private static ulong RequireMutable(RuntimeOverrideStatus status)
    {
        if (status.HasPendingMutation)
        {
            throw new ControlServiceException(
                "NP_SNAPSHOT_ALREADY_PENDING",
                "已有运行模式等待系统组件确认，请稍后再试。");
        }
        return status.ActiveSnapshotVersion
            ?? throw new ControlServiceException(
                "NP_RUNTIME_OVERRIDE_ACTIVE_SNAPSHOT_MISSING",
                "当前没有活动快照，无法切换限时运行模式。");
    }

    private static ProtoMode ToProto(RuntimeOverrideKind kind)
    {
        return kind switch
        {
            RuntimeOverrideKind.Paused => ProtoMode.Paused,
            RuntimeOverrideKind.Direct => ProtoMode.Direct,
            RuntimeOverrideKind.Proxy => ProtoMode.Proxy,
            _ => throw new ArgumentOutOfRangeException(nameof(kind)),
        };
    }

    private static ulong? OptionalVersion(ulong value) => value == 0 ? null : value;

    private static ControlServiceException InvalidContract()
    {
        return new ControlServiceException(
            "NP_CONTROL_CONTRACT_INVALID",
            "控制服务返回了无效的限时运行状态。");
    }
}
