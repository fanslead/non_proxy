using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Bootstrap;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Tests;

internal static class TestPlatformServices
{
    public static ServiceProvider Create(
        PlatformKind platform = PlatformKind.MacOS,
        string displayName = "macOS",
        Action<IServiceCollection>? configure = null)
    {
        return ServiceRegistration.BuildProvider(services =>
        {
            services.AddSingleton<IPlatformInformation>(
                new FakePlatformInformation(platform, displayName));
            services.AddSingleton<ISystemComponentInstaller, FakeSystemComponentInstaller>();
            configure?.Invoke(services);
        });
    }

    private sealed record FakePlatformInformation(
        PlatformKind Platform,
        string DisplayName) : IPlatformInformation;

    private sealed class FakeSystemComponentInstaller : ISystemComponentInstaller
    {
        public Task<SystemComponentState> GetStateAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new SystemComponentState(
                SystemComponentStatus.NotInstalled,
                "测试组件未安装"));
        }

        public Task<InstallResult> InstallAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new InstallResult(true, "测试安装成功"));
        }

        public Task<InstallResult> UninstallAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new InstallResult(true, "测试卸载成功"));
        }

        public Task<InstallResult> OpenSystemSettingsAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new InstallResult(true, "测试设置已打开"));
        }
    }
}
