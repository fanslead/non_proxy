using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class DashboardViewModelTests
{
    [Theory]
    [InlineData(PlatformKind.MacOS, "macOS")]
    [InlineData(PlatformKind.Windows, "Windows")]
    public void PlatformInformationUsesInjectedDisplayName(
        PlatformKind platform,
        string displayName)
    {
        using var services = TestPlatformServices.Create(platform, displayName);

        var viewModel = services.GetRequiredService<DashboardViewModel>();

        Assert.Equal(displayName, viewModel.PlatformLabel);
    }

    [Fact]
    public void InitialStateReportsThatStatusIsLoading()
    {
        using var services = TestPlatformServices.Create();

        var state = services.GetRequiredService<DashboardViewModel>().State;

        Assert.Equal("正在读取系统状态", state.StatusHeadline);
        Assert.Contains("正在检查", state.StatusDetail, StringComparison.Ordinal);
        Assert.Equal(0, state.DirectApplicationCount);
        Assert.Equal(0, state.DirectWebsiteCount);
        Assert.Equal(0, state.DirectNetworkCount);
        Assert.False(state.HasRecentEvidence);
    }

    [Fact]
    public async Task RefreshReportsDisconnectedControlServiceWithoutClaimingProtection()
    {
        using var services = TestPlatformServices.Create();
        var viewModel = services.GetRequiredService<DashboardViewModel>();

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal("等待控制服务", viewModel.State.StatusHeadline);
        Assert.Equal("控制服务未连接", viewModel.State.ConnectionLabel);
        Assert.Equal("系统组件未安装", viewModel.State.ComponentLabel);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public void LiveInterruptionOverridesTheLastSnapshotLabelImmediately()
    {
        using var services = TestPlatformServices.Create();
        var viewModel = services.GetRequiredService<DashboardViewModel>();

        viewModel.SetLiveConnectionState(ConnectionState.Interrupted);

        Assert.Equal("状态更新中断", viewModel.State.ConnectionLabel);
    }

    [Theory]
    [InlineData(PlatformKind.MacOS)]
    [InlineData(PlatformKind.Windows)]
    public void EmergencySemanticsAreSharedAcrossDesktopPlatforms(PlatformKind platform)
    {
        var status = new RuntimeOverrideStatus(
            true,
            null,
            null,
            7,
            null,
            false);
        var outbounds = new OutboundCatalog(
            Array.Empty<OutboundListItem>(),
            1,
            "office");

        var presentation = RuntimeOverridePanelState.Build(
            OptionalRead<RuntimeOverrideStatus>.Success(status),
            OptionalRead<OutboundCatalog>.Success(outbounds));
        var pause = RuntimeOverrideConfirmation.Create(
            RuntimeOverrideKind.Paused,
            null);
        var direct = RuntimeOverrideConfirmation.Create(
            RuntimeOverrideKind.Direct,
            null);

        Assert.True(presentation.CanRequest);
        Assert.True(presentation.CanProxy);
        Assert.Contains("系统路由", pause.Detail, StringComparison.Ordinal);
        Assert.Contains("物理网卡", direct.Detail, StringComparison.Ordinal);
        Assert.NotEqual(pause.Detail, direct.Detail);
        Assert.True(platform is PlatformKind.MacOS or PlatformKind.Windows);
    }

    [Fact]
    public void PendingSnapshotThatCarriesActiveOverrideIsNotPresentedAsANewOverrideRequest()
    {
        var active = new RuntimeOverrideInfo(
            RuntimeOverrideKind.Direct,
            null,
            DateTimeOffset.UtcNow.AddMinutes(5));
        var status = new RuntimeOverrideStatus(
            true,
            active,
            active,
            7,
            8,
            false);

        var presentation = RuntimeOverridePanelState.Build(
            OptionalRead<RuntimeOverrideStatus>.Success(status),
            OptionalRead<OutboundCatalog>.Unavailable);

        Assert.Equal("全部直连仍已生效", presentation.Headline);
        Assert.Contains("其他配置", presentation.Detail, StringComparison.Ordinal);
        Assert.DoesNotContain("新请求", presentation.Detail, StringComparison.Ordinal);
    }

    [Fact]
    public void DefaultGroupDoesNotPretendEmergencyProxyOverrideAcceptsAGroup()
    {
        var status = new RuntimeOverrideStatus(
            true,
            null,
            null,
            7,
            null,
            false);
        var outbounds = new OutboundCatalog(
            [],
            3,
            DefaultOutboundGroupId: "office-failover");

        var presentation = RuntimeOverridePanelState.Build(
            OptionalRead<RuntimeOverrideStatus>.Success(status),
            OptionalRead<OutboundCatalog>.Success(outbounds));

        Assert.True(presentation.CanRequest);
        Assert.False(presentation.CanProxy);
        Assert.Contains("只接受单条默认代理", presentation.Detail, StringComparison.Ordinal);
    }
}
