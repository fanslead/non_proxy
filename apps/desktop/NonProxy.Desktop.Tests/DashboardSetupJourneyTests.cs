using NonProxy.Adapter.V1;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class DashboardSetupJourneyTests
{
    [Fact]
    public void ReadyGatewayRequiresDefaultProxyActiveRulesAndDataPlane()
    {
        var journey = DashboardSetupJourney.Build(
            Overview(dataPlaneEnabled: true, directApplications: 2),
            OptionalRead<OutboundCatalog>.Success(DefaultProxyCatalog()),
            OptionalRead<AdapterCatalog>.Success(EmptyAdapters()));

        Assert.True(journey.Gateway.IsReady);
        Assert.Contains("基础链路已就绪", journey.Gateway.Status, StringComparison.Ordinal);
        Assert.Contains("真实路径", journey.Gateway.ActionLabel, StringComparison.Ordinal);
        Assert.False(journey.Adapter.IsReady);
    }

    [Fact]
    public void GatewayNeverClaimsReadyWhileSnapshotAwaitsProviderAck()
    {
        var journey = DashboardSetupJourney.Build(
            Overview(
                dataPlaneEnabled: false,
                directApplications: 1,
                pendingSnapshotVersion: 8),
            OptionalRead<OutboundCatalog>.Success(DefaultProxyCatalog()),
            OptionalRead<AdapterCatalog>.Success(EmptyAdapters()));

        Assert.True(journey.Gateway.IsPending);
        Assert.Contains("等待数据面确认", journey.Gateway.Status, StringComparison.Ordinal);
    }

    [Fact]
    public void ActiveGatewayDoesNotConfuseExpiredSetupHandshakeWithRuntimeFailure()
    {
        var catalog = DefaultProxyCatalog();
        catalog = catalog with
        {
            Items = catalog.Items.Select(item => item with
            {
                Health = "尚未检查",
                LastCheckedAt = null,
                Latency = null,
                IsHandshakeVerified = false,
            }).ToArray(),
        };

        var journey = DashboardSetupJourney.Build(
            Overview(dataPlaneEnabled: true, directApplications: 1),
            OptionalRead<OutboundCatalog>.Success(catalog),
            OptionalRead<AdapterCatalog>.Success(EmptyAdapters()));

        Assert.True(journey.Gateway.IsReady);
    }

    [Fact]
    public void DraftDirectTargetsDoNotSatisfyActiveGatewayReadiness()
    {
        var journey = DashboardSetupJourney.Build(
            Overview(
                dataPlaneEnabled: true,
                directApplications: 2,
                activeDirectRules: 0),
            OptionalRead<OutboundCatalog>.Success(DefaultProxyCatalog()),
            OptionalRead<AdapterCatalog>.Success(EmptyAdapters()));

        Assert.True(journey.Gateway.IsPending);
        Assert.Contains("尚未激活", journey.Gateway.Status, StringComparison.Ordinal);
    }

    [Fact]
    public void RegisteredAdapterRemainsPendingWithoutPersistedPathEvidence()
    {
        var installation = new AdapterInstallationItem(
            "surge-primary",
            AdapterClient.Surge,
            "Surge",
            "5.11.0",
            "/Applications/Surge.app/Contents/Applications/surge-cli",
            "/tmp/nonproxy.list",
            "/tmp/current.conf",
            null,
            AdapterState.Ready);
        var journey = DashboardSetupJourney.Build(
            Overview(dataPlaneEnabled: false),
            OptionalRead<OutboundCatalog>.Unavailable,
            OptionalRead<AdapterCatalog>.Success(new AdapterCatalog(
                [installation],
                DateTimeOffset.UtcNow)));

        Assert.True(journey.Adapter.IsPending);
        Assert.Contains("真实路径", journey.Adapter.Detail, StringComparison.Ordinal);
        Assert.Contains("已登记", journey.Adapter.Status, StringComparison.Ordinal);
    }

    private static SystemOverview Overview(
        bool dataPlaneEnabled,
        int directApplications = 0,
        ulong? pendingSnapshotVersion = null,
        int? activeDirectRules = null)
    {
        return new SystemOverview(
            ConnectionState.Connected,
            new SystemComponentState(
                SystemComponentStatus.Installed,
                "系统组件已安装"),
            "测试状态",
            "测试详情",
            7,
            directApplications,
            0,
            0,
            4,
            DateTimeOffset.UtcNow,
            pendingSnapshotVersion,
            dataPlaneEnabled,
            activeDirectRules ?? directApplications);
    }

    private static OutboundCatalog DefaultProxyCatalog()
    {
        return new OutboundCatalog(
            [
                new OutboundListItem(
                    "office",
                    "Office Proxy",
                    "SOCKS5",
                    "127.0.0.1:1080",
                    "代理握手可用",
                    TimeSpan.FromMilliseconds(12),
                    DateTimeOffset.UtcNow,
                    IsDefault: true,
                    SupportsDefaultRoute: true,
                    IsHandshakeVerified: true),
            ],
            3,
            "office");
    }

    private static AdapterCatalog EmptyAdapters()
    {
        return new AdapterCatalog([], DateTimeOffset.UtcNow);
    }
}
