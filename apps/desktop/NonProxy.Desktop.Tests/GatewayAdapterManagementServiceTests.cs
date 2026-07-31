using Google.Protobuf;
using NonProxy.Adapter.V1;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;
using NonProxy.Policy.V1;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayAdapterManagementServiceTests
{
    [Fact]
    public async Task SnapshotDriftAfterPreparePreventsClientWrite()
    {
        var adapter = ReadyAdapter();
        var control = new StubControlRpcClient();
        control.ActivePolicySnapshotResponses.Enqueue(Snapshot(7, 1));
        control.ActivePolicySnapshotResponses.Enqueue(Snapshot(8, 2));
        var service = Service(adapter, control);

        var result = await service.SyncAsync(
            "surge-main",
            TestContext.Current.CancellationToken);

        Assert.False(result.Succeeded);
        Assert.Equal("NP_ADAPTER_SNAPSHOT_CHANGED", result.Code);
        Assert.Equal(1, adapter.PrepareCallCount);
        Assert.Equal(0, adapter.ApplyCallCount);
    }

    [Fact]
    public async Task SuccessfulSyncReportsConfigurationWithoutClaimingPath()
    {
        var adapter = ReadyAdapter();
        var control = new StubControlRpcClient
        {
            ActivePolicySnapshotResponse = Snapshot(7, 1),
        };
        var service = Service(adapter, control);

        var result = await service.SyncAsync(
            "surge-main",
            TestContext.Current.CancellationToken);

        Assert.True(result.Succeeded);
        Assert.Equal("NP_ADAPTER_CONFIGURATION_VERIFIED", result.Code);
        Assert.True(result.ClientValidated);
        Assert.True(result.Reloaded);
        Assert.True(result.ConfigurationVerified);
        Assert.False(result.PathVerified);
        Assert.Equal(EvidenceLevel.Configuration, result.EvidenceLevel);
        Assert.Contains("尚未证明", result.Message, StringComparison.Ordinal);
        Assert.Equal(1, adapter.PrepareCallCount);
        Assert.Equal(1, adapter.ApplyCallCount);
    }

    [Fact]
    public async Task UnrepresentableActiveRuleStopsBeforePrepare()
    {
        var adapter = ReadyAdapter();
        var snapshot = Snapshot(7, 1);
        snapshot.Policies[0].Match.Ports.Add(new PortRange
        {
            First = 443,
            Last = 443,
        });
        var control = new StubControlRpcClient
        {
            ActivePolicySnapshotResponse = snapshot,
        };
        var service = Service(adapter, control);

        var result = await service.SyncAsync(
            "surge-main",
            TestContext.Current.CancellationToken);

        Assert.False(result.Succeeded);
        Assert.Equal("NP_ADAPTER_PROJECTION_INCOMPLETE", result.Code);
        Assert.Single(result.Blockers);
        Assert.Equal(0, adapter.PrepareCallCount);
    }

    [Fact]
    public async Task FailedConfigurationVerificationRestoresAndReloadsBackup()
    {
        var adapter = ReadyAdapter();
        adapter.VerifyResponse = new VerifyChangeResponse
        {
            ConfigurationVerified = false,
            PathVerified = false,
        };
        adapter.RollbackResponse = new RollbackChangeResponse
        {
            Restored = true,
            Reloaded = true,
        };
        var control = new StubControlRpcClient
        {
            ActivePolicySnapshotResponse = Snapshot(7, 1),
        };
        var service = Service(adapter, control);

        var result = await service.SyncAsync(
            "surge-main",
            TestContext.Current.CancellationToken);

        Assert.False(result.Succeeded);
        Assert.Equal("NP_ADAPTER_CONFIGURATION_UNVERIFIED", result.Code);
        Assert.Contains("已经恢复", result.Message, StringComparison.Ordinal);
        Assert.Equal(1, adapter.RollbackCallCount);
    }

    [Fact]
    public async Task VerificationTransportFailureStillAttemptsRecovery()
    {
        var adapter = ReadyAdapter();
        adapter.VerifyException = new NonProxy.Desktop.Core.Services.Control.ControlServiceException(
            "NP_ADAPTER_UNAVAILABLE",
            "验证连接已中断。");
        adapter.RollbackResponse = new RollbackChangeResponse
        {
            Restored = true,
            Reloaded = true,
        };
        var control = new StubControlRpcClient
        {
            ActivePolicySnapshotResponse = Snapshot(7, 1),
        };
        var service = Service(adapter, control);

        var result = await service.SyncAsync(
            "surge-main",
            TestContext.Current.CancellationToken);

        Assert.False(result.Succeeded);
        Assert.Equal("NP_ADAPTER_UNAVAILABLE", result.Code);
        Assert.Contains("已经恢复", result.Message, StringComparison.Ordinal);
        Assert.Equal(1, adapter.RollbackCallCount);
    }

    [Fact]
    public async Task UnknownApplyOutcomeStillAttemptsRecovery()
    {
        var adapter = ReadyAdapter();
        adapter.ApplyException = new NonProxy.Desktop.Core.Services.Control.ControlServiceException(
            "NP_ADAPTER_UNAVAILABLE",
            "应用连接已中断。");
        adapter.RollbackResponse = new RollbackChangeResponse
        {
            Restored = true,
            Reloaded = true,
        };
        var control = new StubControlRpcClient
        {
            ActivePolicySnapshotResponse = Snapshot(7, 1),
        };
        var service = Service(adapter, control);

        var result = await service.SyncAsync(
            "surge-main",
            TestContext.Current.CancellationToken);

        Assert.False(result.Succeeded);
        Assert.Equal("NP_ADAPTER_UNAVAILABLE", result.Code);
        Assert.Contains("已经恢复", result.Message, StringComparison.Ordinal);
        Assert.Equal(1, adapter.RollbackCallCount);
    }

    [Fact]
    public async Task IncompleteApplyRecoveryIsRetriedExplicitly()
    {
        var adapter = ReadyAdapter();
        adapter.ApplyResponse = new ApplyChangeResponse
        {
            Error = new ErrorDetail
            {
                Code = "NP_ADAPTER_CLIENT_RELOAD_FAILED",
                Message = "客户端重载失败。",
            },
            Applied = false,
            Reloaded = false,
            RolledBack = true,
            RollbackReloaded = false,
        };
        adapter.RollbackResponse = new RollbackChangeResponse
        {
            Restored = true,
            Reloaded = true,
            Replayed = true,
        };
        var control = new StubControlRpcClient
        {
            ActivePolicySnapshotResponse = Snapshot(7, 1),
        };
        var service = Service(adapter, control);

        var result = await service.SyncAsync(
            "surge-main",
            TestContext.Current.CancellationToken);

        Assert.False(result.Succeeded);
        Assert.Equal("NP_ADAPTER_CLIENT_RELOAD_FAILED", result.Code);
        Assert.Contains("已经恢复", result.Message, StringComparison.Ordinal);
        Assert.Equal(1, adapter.RollbackCallCount);
    }

    private static GatewayAdapterManagementService Service(
        StubAdapterRpcClient adapter,
        StubControlRpcClient control)
    {
        return new GatewayAdapterManagementService(
            adapter,
            control,
            new Catalog(),
            new AdapterPolicyProjector(new MacPlatform()));
    }

    private static StubAdapterRpcClient ReadyAdapter()
    {
        var capabilities = new ReadCapabilitiesResponse();
        capabilities.Capabilities.AddRange([
            AdapterCapability.DomainRule,
            AdapterCapability.HotReload,
        ]);
        return new StubAdapterRpcClient
        {
            CapabilitiesResponse = capabilities,
            PrepareResponse = new PrepareChangeResponse
            {
                ChangeId = "change-123",
                BackupId = "backup-123",
                CandidateHash = ByteString.CopyFrom(Enumerable.Repeat((byte)3, 32)
                    .ToArray()),
                ConfigurationCandidateHash = ByteString.CopyFrom(
                    Enumerable.Repeat((byte)4, 32).ToArray()),
                ClientValidated = true,
                RuleCount = 1,
                ExpiresAt = Google.Protobuf.WellKnownTypes.Timestamp.FromDateTimeOffset(
                    DateTimeOffset.UtcNow.AddMinutes(5)),
                ManagedRulesReference = "./nonproxy.list",
                DirectTarget = "DIRECT",
            },
            ApplyResponse = new ApplyChangeResponse
            {
                Applied = true,
                Reloaded = true,
            },
            VerifyResponse = new VerifyChangeResponse
            {
                ConfigurationVerified = true,
                PathVerified = false,
                EvidenceLevel = EvidenceLevel.Configuration,
            },
        };
    }

    private static GetActivePolicySnapshotResponse Snapshot(
        ulong version,
        byte hashByte)
    {
        var response = new GetActivePolicySnapshotResponse
        {
            SnapshotVersion = version,
            ContentHash = ByteString.CopyFrom(Enumerable.Repeat(hashByte, 32)
                .ToArray()),
        };
        response.Policies.Add(new ProtoPolicy
        {
            Id = "site-example",
            Enabled = true,
            Match = new PolicyMatch
            {
                Domain = new DomainMatcher
                {
                    Kind = DomainMatchKind.Exact,
                    AsciiPattern = "example.com",
                },
            },
            Decision = new DecisionSpec { Action = RouteAction.Direct },
        });
        return response;
    }

    private sealed class Catalog : IApplicationCatalog
    {
        public Task<ApplicationCatalogSnapshot> ListAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new ApplicationCatalogSnapshot(
                Array.Empty<ApplicationCatalogEntry>(),
                true,
                true,
                "ready"));
        }

        public Task<ApplicationSelectionResult> ChooseAsync(
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }
    }

    private sealed class MacPlatform : IPlatformInformation
    {
        public PlatformKind Platform => PlatformKind.MacOS;

        public string DisplayName => "macOS";
    }
}
