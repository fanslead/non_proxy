using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Mac;

internal sealed class MacApplicationCatalog(
    MacNativeBridgeClient nativeBridge) : IApplicationCatalog
{
    public async Task<ApplicationCatalogSnapshot> ListAsync(
        CancellationToken cancellationToken)
    {
        try
        {
            var payload = await nativeBridge.ListApplicationsAsync(
                cancellationToken);
            if (!payload.Success)
            {
                return ApplicationCatalogSnapshot.Unavailable(payload.Message);
            }

            return new ApplicationCatalogSnapshot(
                payload.Applications
                    .Select(Map)
                    .Where(application => application is not null)
                    .Cast<ApplicationCatalogEntry>()
                    .ToArray(),
                true,
                true,
                payload.Message);
        }
        catch (MacNativeBridgeException exception)
        {
            return ApplicationCatalogSnapshot.Unavailable(exception.Message);
        }
    }

    public async Task<ApplicationSelectionResult> ChooseAsync(
        CancellationToken cancellationToken)
    {
        try
        {
            var payload = await nativeBridge.ChooseApplicationAsync(
                cancellationToken);
            if (!payload.Success)
            {
                return new ApplicationSelectionResult(
                    false,
                    null,
                    payload.Message);
            }
            if (payload.Application is null)
            {
                return new ApplicationSelectionResult(
                    true,
                    null,
                    payload.Message);
            }
            var application = Map(payload.Application);
            if (application is null)
            {
                return new ApplicationSelectionResult(
                    false,
                    null,
                    "原生应用选择器返回了无效的应用身份。");
            }
            return new ApplicationSelectionResult(
                true,
                application,
                payload.Message);
        }
        catch (MacNativeBridgeException exception)
        {
            return new ApplicationSelectionResult(
                false,
                null,
                exception.Message);
        }
    }

    private static ApplicationCatalogEntry? Map(
        MacApplicationDescriptor application)
    {
        var bundlePath = NormalizeBundlePath(application.BundlePath);
        if (string.IsNullOrWhiteSpace(application.DisplayName)
            || string.IsNullOrWhiteSpace(application.StableIdentity)
            || bundlePath is null)
        {
            return null;
        }

        return new ApplicationCatalogEntry(
            application.DisplayName.Trim(),
            application.StableIdentity.Trim(),
            NormalizeOptional(application.SignerIdentity),
            NormalizeOptional(application.BundleIdentifier),
            application.IsRunning,
            bundlePath);
    }

    private static string? NormalizeBundlePath(string? value)
    {
        if (string.IsNullOrWhiteSpace(value) || value.Any(char.IsControl))
        {
            return null;
        }

        try
        {
            if (!Path.IsPathFullyQualified(value)
                || !string.Equals(
                    Path.GetExtension(value),
                    ".app",
                    StringComparison.OrdinalIgnoreCase))
            {
                return null;
            }

            var normalized = Path.GetFullPath(value);
            return string.Equals(normalized, value, StringComparison.Ordinal)
                ? normalized
                : null;
        }
        catch (Exception exception) when (
            exception is ArgumentException
                or NotSupportedException
                or PathTooLongException)
        {
            return null;
        }
    }

    private static string? NormalizeOptional(string? value)
    {
        return string.IsNullOrWhiteSpace(value) ? null : value.Trim();
    }
}
