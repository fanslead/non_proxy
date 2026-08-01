using System.Security.Cryptography;
using System.Text.Json;
using NonProxy.Adapter.V1;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Policy.V1;
using ProtoPlatform = NonProxy.Common.V1.Platform;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Core.Services.Adapters;

public sealed class AdapterPolicyProjector(IPlatformInformation platform)
{
    private const int MaximumRules = 4_096;
    private readonly record struct ProjectedRule(
        string Id,
        string Kind,
        string Value,
        string? MatchKind,
        AdapterApplicationProjection? Application = null);

    public AdapterPolicyProjection Project(
        GetActivePolicySnapshotResponse snapshot,
        ApplicationCatalogSnapshot applicationCatalog,
        IReadOnlySet<AdapterCapability> capabilities)
    {
        ArgumentNullException.ThrowIfNull(snapshot);
        ArgumentNullException.ThrowIfNull(applicationCatalog);
        ArgumentNullException.ThrowIfNull(capabilities);
        if (snapshot.SnapshotVersion == 0)
        {
            throw new ControlServiceException(
                "NP_ADAPTER_ACTIVE_SNAPSHOT_REQUIRED",
                "还没有已经生效的策略快照，暂时不能同步第三方客户端。");
        }

        var rules = new List<ProjectedRule>();
        var blockers = new List<AdapterProjectionBlocker>();
        foreach (var policy in snapshot.Policies.OrderBy(value => value.Id, StringComparer.Ordinal))
        {
            ProjectPolicy(policy, applicationCatalog, capabilities, rules, blockers);
        }

        if (rules.Count > MaximumRules)
        {
            throw new ControlServiceException(
                "NP_ADAPTER_RULE_LIMIT_EXCEEDED",
                "可同步的直连规则超过 4096 条，请先精简规则。");
        }

        var payload = WritePayload(snapshot.SnapshotVersion, rules);
        return new AdapterPolicyProjection(
            payload,
            SHA256.HashData(payload),
            rules.Count,
            blockers);
    }

    private void ProjectPolicy(
        ProtoPolicy policy,
        ApplicationCatalogSnapshot applicationCatalog,
        IReadOnlySet<AdapterCapability> capabilities,
        ICollection<ProjectedRule> rules,
        ICollection<AdapterProjectionBlocker> blockers)
    {
        if (!policy.Enabled
            || policy.Decision?.Action != RouteAction.Direct
            || policy.Origin == PolicyOrigin.System
            || policy.SourceKind == PolicySourceKind.System)
        {
            return;
        }
        if (!IsValidIdentifier(policy.Id))
        {
            AddBlocker(
                policy,
                blockers,
                "NP_ADAPTER_POLICY_ID_UNSUPPORTED",
                "规则标识无法安全写入第三方客户端。");
            return;
        }

        var match = policy.Match;
        if (match is null
            || match.Network is not null
            || match.Transports.Count > 0
            || match.Ports.Count > 0)
        {
            AddBlocker(
                policy,
                blockers,
                "NP_ADAPTER_POLICY_SCOPE_UNSUPPORTED",
                "第三方客户端不能无损表达这条直连规则的网络、端口或传输条件。");
            return;
        }

        var dimensions = (match.App is null ? 0 : 1)
            + (match.Domain is null ? 0 : 1)
            + (match.Cidr is null ? 0 : 1);
        if (dimensions != 1)
        {
            AddBlocker(
                policy,
                blockers,
                "NP_ADAPTER_POLICY_COMBINATION_UNSUPPORTED",
                "第三方客户端不能无损表达组合直连条件，已阻止扩大匹配范围。");
            return;
        }

        if (match.App is not null)
        {
            ProjectApplication(
                policy,
                match.App,
                applicationCatalog,
                capabilities,
                rules,
                blockers);
            return;
        }
        if (match.Domain is not null)
        {
            ProjectDomain(
                policy,
                match.Domain,
                capabilities,
                rules,
                blockers);
            return;
        }

        ProjectCidr(
            policy,
            match.Cidr!,
            capabilities,
            rules,
            blockers);
    }

    private void ProjectApplication(
        ProtoPolicy policy,
        AppMatcher matcher,
        ApplicationCatalogSnapshot catalog,
        IReadOnlySet<AdapterCapability> capabilities,
        ICollection<ProjectedRule> rules,
        ICollection<AdapterProjectionBlocker> blockers)
    {
        if (!capabilities.Contains(AdapterCapability.AppRule))
        {
            AddCapabilityBlocker(policy, blockers, "应用规则");
            return;
        }
        if (matcher.Platform != CurrentPlatform()
            || matcher.IncludeHelpers
            || string.IsNullOrWhiteSpace(matcher.StableId))
        {
            AddBlocker(
                policy,
                blockers,
                "NP_ADAPTER_APP_MATCH_UNSUPPORTED",
                "这条应用规则的平台或辅助进程语义不能安全投影。");
            return;
        }
        if (!catalog.IsAvailable)
        {
            AddBlocker(
                policy,
                blockers,
                "NP_ADAPTER_APP_CATALOG_UNAVAILABLE",
                "无法读取受信任的应用目录，因此不能推断应用路径。");
            return;
        }

        var matches = catalog.Applications
            .Where(application =>
                string.Equals(
                    application.StableIdentity,
                    matcher.StableId,
                    StringComparison.Ordinal)
                && (string.IsNullOrWhiteSpace(matcher.SignerId)
                    || string.Equals(
                        application.SignerIdentity,
                        matcher.SignerId,
                        StringComparison.Ordinal)))
            .Select(application => application.AdapterSelector)
            .Where(selector => selector is not null)
            .Cast<ApplicationAdapterSelector>()
            .Distinct(AdapterApplicationSelectorMapper.Comparer)
            .ToArray();
        if (matches.Length != 1)
        {
            AddBlocker(
                policy,
                blockers,
                "NP_ADAPTER_APP_PATH_UNRESOLVED",
                matches.Length == 0
                    ? "没有找到与签名身份匹配的本机应用路径。"
                    : "同一应用身份对应多个路径，请先保留唯一安装项。");
            return;
        }

        var selector = matches[0];
        if (!AdapterApplicationSelectorMapper.TryMap(
                selector,
                platform.Platform,
                out var projection))
        {
            AddBlocker(
                policy,
                blockers,
                selector.Kind == ApplicationAdapterSelectorKind.WindowsPackageFamily
                    ? "NP_ADAPTER_WINDOWS_PACKAGE_UNSUPPORTED"
                    : "NP_ADAPTER_APP_SELECTOR_UNSUPPORTED",
                selector.Kind == ApplicationAdapterSelectorKind.WindowsPackageFamily
                    ? "当前第三方客户端不能按 Windows 包系列身份无损匹配；未退化为进程名。"
                    : "应用目录返回的本机选择器版本或平台不受支持。");
            return;
        }

        rules.Add(new ProjectedRule(
            policy.Id,
            "application",
            projection!.Value,
            null,
            projection));
    }

    private static void ProjectDomain(
        ProtoPolicy policy,
        DomainMatcher matcher,
        IReadOnlySet<AdapterCapability> capabilities,
        ICollection<ProjectedRule> rules,
        ICollection<AdapterProjectionBlocker> blockers)
    {
        if (!capabilities.Contains(AdapterCapability.DomainRule))
        {
            AddCapabilityBlocker(policy, blockers, "域名规则");
            return;
        }
        var matchKind = matcher.Kind switch
        {
            DomainMatchKind.Exact => "exact",
            DomainMatchKind.Suffix or DomainMatchKind.RegistrableDomain => "suffix",
            _ => null,
        };
        if (matchKind is null || string.IsNullOrWhiteSpace(matcher.AsciiPattern))
        {
            AddBlocker(
                policy,
                blockers,
                "NP_ADAPTER_DOMAIN_MATCH_UNSUPPORTED",
                "域名匹配方式无法安全投影到第三方客户端。");
            return;
        }
        rules.Add(new ProjectedRule(
            policy.Id,
            "domain",
            matcher.AsciiPattern,
            matchKind));
    }

    private static void ProjectCidr(
        ProtoPolicy policy,
        CidrMatcher matcher,
        IReadOnlySet<AdapterCapability> capabilities,
        ICollection<ProjectedRule> rules,
        ICollection<AdapterProjectionBlocker> blockers)
    {
        if (!capabilities.Contains(AdapterCapability.CidrRule))
        {
            AddCapabilityBlocker(policy, blockers, "CIDR 规则");
            return;
        }
        if (string.IsNullOrWhiteSpace(matcher.Network))
        {
            AddBlocker(
                policy,
                blockers,
                "NP_ADAPTER_CIDR_MATCH_UNSUPPORTED",
                "CIDR 规则缺少规范网络地址。");
            return;
        }
        rules.Add(new ProjectedRule(
            policy.Id,
            "cidr",
            $"{matcher.Network}/{matcher.PrefixLength}",
            null));
    }

    private static byte[] WritePayload(
        ulong snapshotVersion,
        IEnumerable<ProjectedRule> rules)
    {
        using var stream = new MemoryStream();
        using (var writer = new Utf8JsonWriter(stream))
        {
            writer.WriteStartObject();
            writer.WriteNumber("format_version", 2);
            writer.WriteNumber("revision", snapshotVersion);
            writer.WriteStartArray("rules");
            foreach (var rule in rules)
            {
                writer.WriteStartObject();
                writer.WriteString("id", rule.Id);
                writer.WriteString("action", "direct");
                writer.WriteStartObject("selector");
                writer.WriteString("kind", rule.Kind);
                if (rule.MatchKind is not null)
                {
                    writer.WriteString("match_kind", rule.MatchKind);
                }
                if (rule.Application is { } application)
                {
                    writer.WriteNumber("selector_version", application.Version);
                    writer.WriteString("platform", application.Platform);
                    writer.WriteString("path_kind", application.PathKind);
                }
                writer.WriteString("value", rule.Value);
                writer.WriteEndObject();
                writer.WriteEndObject();
            }
            writer.WriteEndArray();
            writer.WriteEndObject();
        }
        return stream.ToArray();
    }

    private ProtoPlatform CurrentPlatform()
    {
        return platform.Platform switch
        {
            PlatformKind.MacOS => ProtoPlatform.Macos,
            PlatformKind.Windows => ProtoPlatform.Windows,
            _ => ProtoPlatform.Unspecified,
        };
    }

    private static bool IsValidIdentifier(string value)
    {
        return !string.IsNullOrWhiteSpace(value)
            && value.Length <= 128
            && value.All(character =>
                char.IsAsciiLetterOrDigit(character)
                || character is '.' or '_' or '-');
    }

    private static void AddCapabilityBlocker(
        ProtoPolicy policy,
        ICollection<AdapterProjectionBlocker> blockers,
        string capability)
    {
        AddBlocker(
            policy,
            blockers,
            "NP_ADAPTER_CAPABILITY_MISSING",
            $"当前客户端版本不支持{capability}。");
    }

    private static void AddBlocker(
        ProtoPolicy policy,
        ICollection<AdapterProjectionBlocker> blockers,
        string code,
        string message)
    {
        blockers.Add(new AdapterProjectionBlocker(policy.Id, code, message));
    }
}
