namespace NonProxy.Desktop.Core.Platform;

public interface ISystemComponentInstaller
{
    Task<SystemComponentState> GetStateAsync(CancellationToken cancellationToken);

    Task<InstallResult> InstallAsync(CancellationToken cancellationToken);

    Task<InstallResult> UninstallAsync(CancellationToken cancellationToken);
}
