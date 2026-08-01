using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using ProtoPlatform = NonProxy.Common.V1.Platform;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayActivityService : IActivityService
{
    private const int MaximumStableIdentityCharacters = 2048;
    private const int MaximumShortIdentityCharacters = 512;
    private readonly IControlRpcClient _client;

    public GatewayActivityService(IControlRpcClient client)
    {
        _client = client;
    }

    public async Task<IReadOnlyList<ActivityItem>> GetRecentAsync(
        int limit,
        CancellationToken cancellationToken)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(limit, 1);
        ArgumentOutOfRangeException.ThrowIfGreaterThan(limit, 200);
        var response = await _client.ListConnectionDecisionsAsync(
            limit,
            string.Empty,
            cancellationToken);
        return [.. response.Decisions.Select(Map)];
    }

    private static ActivityItem Map(ConnectionDecisionSummary value)
    {
        ValidateEvidence(value);
        var platform = MapPlatform(value.AppPlatform);
        var stableId = RequiredIdentity(
            value.AppStableId,
            "稳定身份",
            MaximumStableIdentityCharacters);
        var signerId = OptionalIdentity(
            value.AppSignerId,
            "签名身份",
            MaximumShortIdentityCharacters);
        var parentStableId = OptionalIdentity(
            value.AppParentStableId,
            "父应用身份",
            MaximumShortIdentityCharacters);
        var helperGroupId = OptionalIdentity(
            value.AppHelperGroupId,
            "辅助进程组身份",
            MaximumShortIdentityCharacters);
        var action = ActionLabel(value);
        var evidence = EvidenceLabel(value.EvidenceLevel);
        var occurredAt = value.ObservedAt?.ToDateTimeOffset()
            ?? throw new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "活动记录缺少观测时间，请重启控制服务后重试。");
        return new ActivityItem(
            checked((long)value.Sequence),
            occurredAt,
            platform,
            stableId,
            signerId,
            parentStableId,
            helperGroupId,
            value.ReasonCode == "NP_POLICY_SYSTEM_MATCH",
            DisplayApplication(value, stableId),
            $"{value.Destination} · {value.DestinationPort}",
            action,
            ReasonLabel(value),
            evidence,
            PathLabel(value),
            value.ErrorCode,
            value.SnapshotVersion);
    }

    private static string DisplayApplication(
        ConnectionDecisionSummary value,
        string stableId)
    {
        return string.IsNullOrWhiteSpace(value.AppDisplayName)
            ? stableId
            : value.AppDisplayName.Trim();
    }

    private static PlatformKind MapPlatform(ProtoPlatform value)
    {
        return value switch
        {
            ProtoPlatform.Macos => PlatformKind.MacOS,
            ProtoPlatform.Windows => PlatformKind.Windows,
            _ => throw InvalidEvidenceContract("活动记录的应用平台无效。"),
        };
    }

    private static string RequiredIdentity(
        string value,
        string label,
        int maximumCharacters)
    {
        return OptionalIdentity(value, label, maximumCharacters)
            ?? throw InvalidEvidenceContract($"活动记录缺少应用{label}。");
    }

    private static string? OptionalIdentity(
        string value,
        string label,
        int maximumCharacters)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return null;
        }

        var normalized = value.Trim();
        if (normalized.Length != value.Length
            || normalized.Length > maximumCharacters
            || normalized.Any(char.IsControl))
        {
            throw InvalidEvidenceContract($"活动记录的应用{label}无效。");
        }
        return normalized;
    }

    private static string ActionLabel(ConnectionDecisionSummary value)
    {
        if (value.FailOpenDirect)
        {
            return "代理失败→直连";
        }
        return value.Action switch
        {
            RouteAction.Direct => "直连",
            RouteAction.Proxy => "代理",
            RouteAction.Block => "阻止",
            _ => throw InvalidEvidenceContract("活动记录的路由动作无效。"),
        };
    }

    private static string EvidenceLabel(EvidenceLevel evidence)
    {
        return evidence switch
        {
            EvidenceLevel.Decision => "仅策略决策",
            EvidenceLevel.Path => "路径已确认",
            EvidenceLevel.Exit => "公网出口已验证",
            _ => throw InvalidEvidenceContract("活动记录的证据等级无效。"),
        };
    }

    private static string ReasonLabel(ConnectionDecisionSummary value)
    {
        if (!string.IsNullOrWhiteSpace(value.MatchedRuleId))
        {
            return $"命中规则 {value.MatchedRuleId}（{value.ReasonCode}）";
        }

        if (!string.IsNullOrWhiteSpace(value.MatchedPolicyId))
        {
            return $"命中策略 {value.MatchedPolicyId}（{value.ReasonCode}）";
        }

        return value.ReasonCode == "NP_POLICY_DEFAULT"
            ? "使用默认路由"
            : value.ReasonCode;
    }

    private static string PathLabel(ConnectionDecisionSummary value)
    {
        if (value.Action == RouteAction.Block)
        {
            return "策略已阻止，未建立数据路径";
        }

        if (value.EvidenceLevel == EvidenceLevel.Decision)
        {
            return "尚未确认实际数据路径";
        }

        var path = value.Action == RouteAction.Direct || value.FailOpenDirect
            ? $"物理接口 {value.InterfaceName}"
            : $"代理出口 {value.OutboundId}";
        if (value.FailOpenDirect)
        {
            path = $"{path}（fail-open 回退）";
        }
        return value.EvidenceLevel == EvidenceLevel.Exit
            ? $"{path}，出口探针 {value.ExitProbeId}"
            : path;
    }

    private static void ValidateEvidence(ConnectionDecisionSummary value)
    {
        var hasInterface = !string.IsNullOrWhiteSpace(value.InterfaceName);
        var hasOutbound = !string.IsNullOrWhiteSpace(value.OutboundId);
        var hasProbe = !string.IsNullOrWhiteSpace(value.ExitProbeId);
        var pathMatches = !value.FailOpenDirect && value.Action switch
        {
            RouteAction.Direct => hasInterface && !hasOutbound,
            RouteAction.Proxy => !hasInterface && hasOutbound,
            RouteAction.Block => false,
            _ => false,
        };
        var fallbackMatches = value.FailOpenDirect
            && value.Action == RouteAction.Proxy
            && value.FailureMode == FailureMode.Open
            && hasInterface
            && !hasOutbound
            && !string.IsNullOrEmpty(value.ErrorCode);
        var valid = value.EvidenceLevel switch
        {
            EvidenceLevel.Decision => !value.FailOpenDirect
                && !hasInterface
                && !hasOutbound
                && !hasProbe,
            EvidenceLevel.Path => (pathMatches || fallbackMatches) && !hasProbe,
            EvidenceLevel.Exit => (pathMatches || fallbackMatches) && hasProbe,
            _ => false,
        };
        if (!valid || (!string.IsNullOrEmpty(value.ErrorCode)
            && value.EvidenceLevel != EvidenceLevel.Decision
            && !value.FailOpenDirect))
        {
            throw InvalidEvidenceContract("活动记录的路径证据与路由动作不一致。");
        }
    }

    private static ControlServiceException InvalidEvidenceContract(string message)
    {
        return new ControlServiceException("NP_CONTROL_CONTRACT_INVALID", message);
    }
}
