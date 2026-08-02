using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Policy.V1;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class PolicyContractMapper
{
    private readonly IPlatformInformation _platform;

    public PolicyContractMapper(IPlatformInformation platform)
    {
        _platform = platform;
    }

    public (ProtoPolicy Policy, ulong ExpectedRevision) ToContract(PolicyDraft draft)
    {
        ArgumentNullException.ThrowIfNull(draft);
        var revision = NextRevision(draft);
        var matcher = CreateMatcher(draft);
        var outboundId = draft.OutboundId?.Trim() ?? string.Empty;
        if (draft.Action == PolicyAction.Proxy && string.IsNullOrWhiteSpace(outboundId))
        {
            throw new ControlServiceException(
                "NP_POLICY_OUTBOUND_REQUIRED",
                "代理规则必须选择一个已配置出口。");
        }

        var decision = new DecisionSpec
        {
            Action = ToRouteAction(draft.Action),
            FailureMode = FailureMode.Closed,
        };
        if (draft.Action == PolicyAction.Proxy)
        {
            decision.OutboundId = outboundId;
        }

        var policy = new ProtoPolicy
        {
            Id = draft.ExistingId ?? $"user-{Guid.NewGuid():N}",
            DisplayName = draft.Name.Trim(),
            SourceKind = ToSourceKind(draft.Scope),
            Match = matcher,
            Decision = decision,
            Priority = 100,
            Enabled = true,
            Origin = PolicyOrigin.User,
            Revision = revision,
        };
        return (policy, draft.ExistingRevision ?? 0);
    }

    public static PolicyListItem FromStatus(PolicyStatus status)
    {
        ArgumentNullException.ThrowIfNull(status);
        var policy = status.Policy
            ?? throw InvalidContract("规则状态缺少 policy。");
        var (scope, matchValue) = ReadMatcher(policy.Match);
        return new PolicyListItem(
            policy.Id,
            policy.DisplayName,
            scope,
            matchValue,
            FromAction(policy.Decision?.Action ?? RouteAction.Unspecified),
            FromRuntimeState(status.State),
            OptionalVersion(status.TargetSnapshotVersion),
            policy.UpdatedAt?.ToDateTimeOffset(),
            policy.Revision,
            OptionalVersion(status.EffectiveRevision),
            OptionalVersion(status.PendingRevision));
    }

    public static PolicyListItem FromLegacy(ProtoPolicy policy)
    {
        ArgumentNullException.ThrowIfNull(policy);
        var (scope, matchValue) = ReadMatcher(policy.Match);
        return new PolicyListItem(
            policy.Id,
            policy.DisplayName,
            scope,
            matchValue,
            FromAction(policy.Decision?.Action ?? RouteAction.Unspecified),
            PolicyApplyState.Draft,
            null,
            policy.UpdatedAt?.ToDateTimeOffset(),
            policy.Revision);
    }

    private static ulong NextRevision(PolicyDraft draft)
    {
        if (draft.ExistingId is null)
        {
            return 1;
        }

        if (draft.ExistingRevision is not { } current || current == ulong.MaxValue)
        {
            throw new ControlServiceException(
                "NP_POLICY_REVISION_REQUIRED",
                "编辑规则时缺少有效修订号，请刷新后重试。");
        }

        return current + 1;
    }

    private PolicyMatch CreateMatcher(PolicyDraft draft)
    {
        var matcher = new PolicyMatch();
        if (draft.Scope is PolicyScope.Application
            or PolicyScope.ApplicationAndDestination)
        {
            matcher.App = new AppMatcher
            {
                Platform = ToPlatform(_platform.Platform),
                StableId = draft.MatchValue.Trim(),
                SignerId = draft.ApplicationSignerId?.Trim() ?? string.Empty,
                IncludeHelpers = draft.IncludeApplicationHelpers,
            };
        }

        if (draft.Scope == PolicyScope.Website)
        {
            matcher.Domain = ExactDomain(draft.MatchValue);
        }
        else if (draft.Scope == PolicyScope.Network)
        {
            if (string.IsNullOrWhiteSpace(draft.MatchValue))
            {
                throw new ControlServiceException(
                    "NP_POLICY_NETWORK_REQUIRED",
                    "网络规则必须选择一个已保存的网络配置。");
            }

            matcher.Network = new NetworkMatcher
            {
                ProfileId = draft.MatchValue.Trim(),
            };
        }
        else if (draft.Scope == PolicyScope.ApplicationAndDestination)
        {
            if (string.IsNullOrWhiteSpace(draft.Destination))
            {
                throw new ControlServiceException(
                    "NP_POLICY_DESTINATION_REQUIRED",
                    "应用加目标规则必须填写目标域名。");
            }

            matcher.Domain = ExactDomain(draft.Destination);
        }

        return matcher;
    }

    private static DomainMatcher ExactDomain(string value)
    {
        return new DomainMatcher
        {
            Kind = DomainMatchKind.Exact,
            AsciiPattern = value.Trim(),
        };
    }

    private static NonProxy.Common.V1.Platform ToPlatform(PlatformKind platform)
    {
        return platform switch
        {
            PlatformKind.MacOS => NonProxy.Common.V1.Platform.Macos,
            PlatformKind.Windows => NonProxy.Common.V1.Platform.Windows,
            _ => throw new ControlServiceException(
                "NP_PLATFORM_UNSUPPORTED",
                "当前平台无法创建按应用匹配的规则。"),
        };
    }

    private static PolicySourceKind ToSourceKind(PolicyScope scope)
    {
        return scope switch
        {
            PolicyScope.Application => PolicySourceKind.App,
            PolicyScope.Website => PolicySourceKind.Site,
            PolicyScope.ApplicationAndDestination => PolicySourceKind.AppDestination,
            PolicyScope.Network => PolicySourceKind.Network,
            _ => throw InvalidContract("未知规则范围。"),
        };
    }

    private static RouteAction ToRouteAction(PolicyAction action)
    {
        return action switch
        {
            PolicyAction.Direct => RouteAction.Direct,
            PolicyAction.Proxy => RouteAction.Proxy,
            PolicyAction.Block => RouteAction.Block,
            _ => throw InvalidContract("未知规则动作。"),
        };
    }

    private static PolicyAction FromAction(RouteAction action)
    {
        return action switch
        {
            RouteAction.Direct => PolicyAction.Direct,
            RouteAction.Proxy => PolicyAction.Proxy,
            RouteAction.Block => PolicyAction.Block,
            _ => throw InvalidContract("控制服务返回了未知规则动作。"),
        };
    }

    private static PolicyApplyState FromRuntimeState(PolicyRuntimeState state)
    {
        return state switch
        {
            PolicyRuntimeState.Draft => PolicyApplyState.Draft,
            PolicyRuntimeState.Pending => PolicyApplyState.Pending,
            PolicyRuntimeState.Active => PolicyApplyState.Active,
            PolicyRuntimeState.PendingRemoval => PolicyApplyState.PendingRemoval,
            PolicyRuntimeState.Rejected => PolicyApplyState.Rejected,
            _ => throw InvalidContract("控制服务返回了未知规则状态。"),
        };
    }

    private static (PolicyScope Scope, string MatchValue) ReadMatcher(PolicyMatch? matcher)
    {
        if (matcher?.Network is not null)
        {
            if (matcher.App is not null
                || matcher.Domain is not null
                || matcher.Cidr is not null)
            {
                throw InvalidContract("控制服务返回了桌面端尚不支持的组合网络规则。");
            }

            if (string.IsNullOrWhiteSpace(matcher.Network.ProfileId))
            {
                throw InvalidContract("控制服务返回了缺少配置档的网络规则。");
            }

            return (PolicyScope.Network, matcher.Network.ProfileId);
        }

        if (matcher?.App is not null && matcher.Domain is not null)
        {
            return (PolicyScope.ApplicationAndDestination, matcher.App.StableId);
        }

        if (matcher?.App is not null)
        {
            return (PolicyScope.Application, matcher.App.StableId);
        }

        if (matcher?.Domain is not null)
        {
            return (PolicyScope.Website, matcher.Domain.AsciiPattern);
        }

        throw InvalidContract("控制服务返回了桌面端尚不支持的规则范围。");
    }

    private static ulong? OptionalVersion(ulong value)
    {
        return value == 0 ? null : value;
    }

    private static ControlServiceException InvalidContract(string message)
    {
        return new ControlServiceException("NP_CONTROL_CONTRACT_INVALID", message);
    }
}
