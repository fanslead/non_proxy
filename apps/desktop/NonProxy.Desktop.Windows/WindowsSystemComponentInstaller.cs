using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Windows;

internal sealed class WindowsSystemComponentInstaller : ISystemComponentInstaller
{
    private const string NotPackagedErrorCode = "NP_PLATFORM_COMPONENT_NOT_PACKAGED";

    public Task<SystemComponentState> GetStateAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(new SystemComponentState(
            SystemComponentStatus.Unavailable,
            "当前构建尚未包含签名后的 Windows Service 与 WFP 组件。",
            NotPackagedErrorCode));
    }

    public Task<InstallResult> InstallAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(InstallResult.Unavailable(
            "无法安装：当前构建尚未包含签名后的 Windows Service 与 WFP 组件。",
            NotPackagedErrorCode));
    }

    public Task<InstallResult> UninstallAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(InstallResult.Unavailable(
            "无需卸载：当前构建尚未包含签名后的 Windows Service 与 WFP 组件。",
            NotPackagedErrorCode));
    }
}
