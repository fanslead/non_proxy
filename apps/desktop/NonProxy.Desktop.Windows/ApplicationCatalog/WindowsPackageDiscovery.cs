#if WINDOWS
using System.Runtime.Versioning;
using Windows.ApplicationModel;
using Windows.Management.Deployment;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

[SupportedOSPlatform("windows10.0.18362.0")]
internal sealed class WindowsPackageDiscovery : IWindowsPackageDiscovery
{
    private const int MaximumPackages = 1024;

    public WindowsPackageDiscoverySnapshot Discover(
        CancellationToken cancellationToken)
    {
        try
        {
            var candidates = new Dictionary<string, WindowsApplicationCandidate>(
                StringComparer.OrdinalIgnoreCase);
            var manager = new PackageManager();
            foreach (var package in manager.FindPackagesForUser(string.Empty))
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (candidates.Count >= MaximumPackages)
                {
                    break;
                }
                try
                {
                    AddPackage(candidates, package, cancellationToken);
                }
                catch (OperationCanceledException)
                {
                    throw;
                }
                catch (Exception exception) when (IsExpectedFailure(exception))
                {
                    // One broken package must not hide the rest of the catalog.
                }
            }
            return new WindowsPackageDiscoverySnapshot(
                candidates.Values.ToArray(),
                true);
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (Exception exception) when (IsExpectedFailure(exception))
        {
            return new WindowsPackageDiscoverySnapshot([], false);
        }
    }

    private static void AddPackage(
        Dictionary<string, WindowsApplicationCandidate> candidates,
        Package package,
        CancellationToken cancellationToken)
    {
        if (package.IsFramework
            || package.IsResourcePackage
            || package.IsBundle
            || (OperatingSystem.IsWindowsVersionAtLeast(10, 0, 19041)
                && package.IsStub)
            || !package.Status.VerifyIsOK())
        {
            return;
        }
        var entries = package.GetAppListEntriesAsync()
            .AsTask(cancellationToken)
            .GetAwaiter()
            .GetResult();
        if (entries.Count == 0)
        {
            return;
        }
        var familyName = package.Id.FamilyName;
        var publisherId = package.Id.PublisherId;
        if (string.IsNullOrWhiteSpace(familyName)
            || string.IsNullOrWhiteSpace(publisherId))
        {
            return;
        }
        var displayName = entries
            .Select(entry => entry.DisplayInfo.DisplayName)
            .FirstOrDefault(value => !string.IsNullOrWhiteSpace(value));
        candidates.TryAdd(
            familyName,
            new WindowsApplicationCandidate(
                familyName,
                WindowsApplicationDiscovery.CleanPackageDisplayName(
                    displayName ?? package.DisplayName,
                    package.Id.Name),
                false,
                WindowsApplicationCandidateKind.Package,
                publisherId));
    }

    private static bool IsExpectedFailure(Exception exception)
    {
        return exception is UnauthorizedAccessException
            or InvalidOperationException
            or ArgumentException
            or NotSupportedException
            or System.Runtime.InteropServices.COMException;
    }
}
#else
namespace NonProxy.Desktop.Windows.ApplicationCatalog;

internal sealed class WindowsPackageDiscovery : IWindowsPackageDiscovery
{
    public WindowsPackageDiscoverySnapshot Discover(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return new WindowsPackageDiscoverySnapshot([], false);
    }
}
#endif
