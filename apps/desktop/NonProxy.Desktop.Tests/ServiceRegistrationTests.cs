using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Bootstrap;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Features.Shell;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Tests;

public sealed class ServiceRegistrationTests
{
    [Fact]
    public void MissingInstallerThrowsDuringComposition()
    {
        var exception = Assert.Throws<InvalidOperationException>(() =>
            ServiceRegistration.BuildProvider(services =>
                services.AddSingleton<IPlatformInformation>(
                    new StubPlatformInformation())));

        Assert.Contains(
            nameof(ISystemComponentInstaller),
            exception.Message,
            StringComparison.Ordinal);
    }

    [Fact]
    public void CompletePlatformServicesResolvesShell()
    {
        using var services = TestPlatformServices.Create();

        var shell = services.GetRequiredService<MainWindowViewModel>();

        Assert.Equal("macOS", shell.PlatformLabel);
        Assert.Same(
            services.GetRequiredService<DashboardViewModel>(),
            shell.Dashboard);
        Assert.Equal(9, shell.NavigationItems.Count);
        Assert.Equal("运行概览", shell.CurrentPage.Title);
    }

    [Fact]
    public async Task PlatformWithoutDiscoveryReportsTheBoundaryExplicitly()
    {
        using var services = TestPlatformServices.Create(
            PlatformKind.Windows,
            "Windows");
        var discovery = services.GetRequiredService<ILocalProxyDiscovery>();

        var result = await discovery.DiscoverAsync(
            TestContext.Current.CancellationToken);

        Assert.False(result.IsAvailable);
        Assert.Empty(result.Candidates);
        Assert.Contains("尚未接入", result.Message, StringComparison.Ordinal);
    }

    private sealed class StubPlatformInformation : IPlatformInformation
    {
        public PlatformKind Platform => PlatformKind.Unknown;

        public string DisplayName => "测试平台";
    }
}
