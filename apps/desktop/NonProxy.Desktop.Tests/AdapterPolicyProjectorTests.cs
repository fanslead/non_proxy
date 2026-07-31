using System.Security.Cryptography;
using System.Text.Json;
using NonProxy.Adapter.V1;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;
using NonProxy.Policy.V1;
using ProtoPlatform = NonProxy.Common.V1.Platform;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Tests;

public sealed class AdapterPolicyProjectorTests
{
    private static readonly IReadOnlySet<AdapterCapability> AllCapabilities =
        new HashSet<AdapterCapability>
        {
            AdapterCapability.AppRule,
            AdapterCapability.DomainRule,
            AdapterCapability.CidrRule,
            AdapterCapability.HotReload,
        };

    [Fact]
    public void ProjectsOnlyExactDirectSelectorsWithDeterministicHash()
    {
        var snapshot = Snapshot(
            ApplicationPolicy("app-office", "com.example.office", "TEAM123"),
            DomainPolicy("site-example", DomainMatchKind.Exact, "example.com"),
            CidrPolicy("cidr-private", "10.0.0.0", 8),
            DomainPolicy(
                "proxy-ignored",
                DomainMatchKind.Exact,
                "proxy.example",
                RouteAction.Proxy));
        var applications = Catalog(new ApplicationCatalogEntry(
            "Office",
            "com.example.office",
            "TEAM123",
            "com.example.office",
            true,
            "/Applications/Office.app"));

        var result = Projector().Project(
            snapshot,
            applications,
            AllCapabilities);

        Assert.Empty(result.Blockers);
        Assert.Equal(3, result.RuleCount);
        Assert.Equal(SHA256.HashData(result.Payload), result.PayloadHash);
        using var document = JsonDocument.Parse(result.Payload);
        var root = document.RootElement;
        Assert.Equal(1, root.GetProperty("format_version").GetInt32());
        Assert.Equal<ulong>(7, root.GetProperty("revision").GetUInt64());
        var rules = root.GetProperty("rules").EnumerateArray().ToArray();
        Assert.Equal(
            ["app-office", "cidr-private", "site-example"],
            rules.Select(rule => rule.GetProperty("id").GetString()!).ToArray());
        Assert.Equal(
            "/Applications/Office.app",
            rules[0].GetProperty("selector").GetProperty("bundle_path").GetString());
        Assert.Equal(
            "10.0.0.0/8",
            rules[1].GetProperty("selector").GetProperty("value").GetString());
        Assert.Equal(
            "exact",
            rules[2].GetProperty("selector").GetProperty("match_kind").GetString());
    }

    [Fact]
    public void CombinedRuleBlocksProjectionInsteadOfWideningIt()
    {
        var policy = ApplicationPolicy(
            "office-login",
            "com.example.office",
            "TEAM123");
        policy.Match.Domain = new DomainMatcher
        {
            Kind = DomainMatchKind.Exact,
            AsciiPattern = "login.example.com",
        };

        var result = Projector().Project(
            Snapshot(policy),
            Catalog(new ApplicationCatalogEntry(
                "Office",
                "com.example.office",
                "TEAM123",
                null,
                false,
                "/Applications/Office.app")),
            AllCapabilities);

        Assert.Equal(0, result.RuleCount);
        var blocker = Assert.Single(result.Blockers);
        Assert.Equal("NP_ADAPTER_POLICY_COMBINATION_UNSUPPORTED", blocker.Code);
    }

    [Fact]
    public void ApplicationRequiresOneSignedCatalogPath()
    {
        var result = Projector().Project(
            Snapshot(ApplicationPolicy(
                "app-office",
                "com.example.office",
                "TEAM123")),
            Catalog(new ApplicationCatalogEntry(
                "Impostor",
                "com.example.office",
                "OTHER",
                null,
                false,
                "/Applications/Impostor.app")),
            AllCapabilities);

        Assert.Empty(JsonDocument.Parse(result.Payload)
            .RootElement.GetProperty("rules").EnumerateArray());
        Assert.Equal(
            "NP_ADAPTER_APP_PATH_UNRESOLVED",
            Assert.Single(result.Blockers).Code);
    }

    [Fact]
    public void InternalSystemPolicyIsNotExportedToThirdPartyClient()
    {
        var system = ApplicationPolicy(
            "system-macos-gateway-direct",
            "com.nonproxy.gatewayd",
            "TEAM123");
        system.Origin = PolicyOrigin.System;
        system.SourceKind = PolicySourceKind.System;

        var result = Projector().Project(
            Snapshot(system, DomainPolicy(
                "site-example",
                DomainMatchKind.Exact,
                "example.com")),
            ApplicationCatalogSnapshot.Unavailable("not needed"),
            AllCapabilities);

        Assert.Empty(result.Blockers);
        Assert.Equal(1, result.RuleCount);
        using var document = JsonDocument.Parse(result.Payload);
        Assert.Equal(
            "site-example",
            document.RootElement.GetProperty("rules")[0]
                .GetProperty("id").GetString());
    }

    private static AdapterPolicyProjector Projector()
    {
        return new AdapterPolicyProjector(new MacPlatform());
    }

    private static GetActivePolicySnapshotResponse Snapshot(params ProtoPolicy[] policies)
    {
        var response = new GetActivePolicySnapshotResponse
        {
            SnapshotVersion = 7,
            ContentHash = Google.Protobuf.ByteString.CopyFrom(new byte[32]),
        };
        response.Policies.AddRange(policies);
        return response;
    }

    private static ApplicationCatalogSnapshot Catalog(
        params ApplicationCatalogEntry[] applications)
    {
        return new ApplicationCatalogSnapshot(
            applications,
            true,
            true,
            "ready");
    }

    private static ProtoPolicy ApplicationPolicy(
        string id,
        string stableId,
        string signerId)
    {
        return Policy(id, new PolicyMatch
        {
            App = new AppMatcher
            {
                Platform = ProtoPlatform.Macos,
                StableId = stableId,
                SignerId = signerId,
            },
        });
    }

    private static ProtoPolicy DomainPolicy(
        string id,
        DomainMatchKind kind,
        string domain,
        RouteAction action = RouteAction.Direct)
    {
        return Policy(id, new PolicyMatch
        {
            Domain = new DomainMatcher
            {
                Kind = kind,
                AsciiPattern = domain,
            },
        }, action);
    }

    private static ProtoPolicy CidrPolicy(
        string id,
        string network,
        uint prefixLength)
    {
        return Policy(id, new PolicyMatch
        {
            Cidr = new CidrMatcher
            {
                Network = network,
                PrefixLength = prefixLength,
            },
        });
    }

    private static ProtoPolicy Policy(
        string id,
        PolicyMatch match,
        RouteAction action = RouteAction.Direct)
    {
        return new ProtoPolicy
        {
            Id = id,
            Enabled = true,
            Match = match,
            Decision = new DecisionSpec { Action = action },
        };
    }

    private sealed class MacPlatform : IPlatformInformation
    {
        public PlatformKind Platform => PlatformKind.MacOS;

        public string DisplayName => "macOS";
    }
}
