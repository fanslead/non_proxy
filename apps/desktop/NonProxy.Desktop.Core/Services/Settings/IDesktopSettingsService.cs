namespace NonProxy.Desktop.Core.Services.Settings;

public interface IDesktopSettingsService
{
    Task<DesktopSettings> GetAsync(CancellationToken cancellationToken);

    Task SaveAsync(
        DesktopSettings settings,
        CancellationToken cancellationToken);
}
