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
    public void InitialStateHasHonestUnconfiguredValues()
    {
        using var services = TestPlatformServices.Create();

        var state = services.GetRequiredService<DashboardViewModel>().State;

        Assert.Equal("等待系统组件", state.StatusHeadline);
        Assert.Contains("不会接管任何网络流量", state.StatusDetail, StringComparison.Ordinal);
        Assert.Equal(0, state.DirectApplicationCount);
        Assert.Equal(0, state.DirectWebsiteCount);
        Assert.False(state.HasRecentEvidence);
    }
}
