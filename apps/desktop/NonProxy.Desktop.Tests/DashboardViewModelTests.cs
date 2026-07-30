using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Platform;

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
}
