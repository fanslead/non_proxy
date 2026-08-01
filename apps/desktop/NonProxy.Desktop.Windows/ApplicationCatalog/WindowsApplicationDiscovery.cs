using System.Diagnostics;
using System.Runtime.Versioning;
using Microsoft.Win32;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

[SupportedOSPlatform("windows")]
internal sealed class WindowsApplicationDiscovery(
    IWindowsPackageDiscovery packageDiscovery) : IWindowsApplicationDiscovery
{
    private const int MaximumCandidates = 1024;
    private const string AppPaths =
        @"Software\Microsoft\Windows\CurrentVersion\App Paths";

    public WindowsApplicationDiscoverySnapshot Discover(
        CancellationToken cancellationToken)
    {
        var candidates = new Dictionary<string, WindowsApplicationCandidate>(
            StringComparer.OrdinalIgnoreCase);
        AddRunning(candidates, cancellationToken);
        AddRegistered(candidates, cancellationToken);
        var packageSnapshot = packageDiscovery.Discover(cancellationToken);
        foreach (var candidate in packageSnapshot.Candidates)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (candidates.Count >= MaximumCandidates)
            {
                break;
            }
            AddCandidate(candidates, candidate);
        }
        return new WindowsApplicationDiscoverySnapshot(
            candidates.Values.ToArray(),
            packageSnapshot.IsAvailable);
    }

    private static void AddRunning(
        Dictionary<string, WindowsApplicationCandidate> candidates,
        CancellationToken cancellationToken)
    {
        using var currentProcess = Process.GetCurrentProcess();
        var currentSessionId = currentProcess.SessionId;
        foreach (var process in Process.GetProcesses())
        {
            using (process)
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (candidates.Count >= MaximumCandidates)
                {
                    return;
                }
                try
                {
                    if (process.SessionId != currentSessionId)
                    {
                        continue;
                    }
                    var path = NormalizeExecutablePath(process.MainModule?.FileName);
                    if (path is not null)
                    {
                        AddCandidate(candidates, new WindowsApplicationCandidate(
                            path,
                            CleanDisplayName(process.ProcessName, path),
                            true));
                    }
                }
                catch (Exception exception) when (
                    exception is InvalidOperationException
                        or System.ComponentModel.Win32Exception
                        or NotSupportedException)
                {
                    // Protected and short-lived processes are intentionally skipped.
                }
            }
        }
    }

    private static void AddRegistered(
        Dictionary<string, WindowsApplicationCandidate> candidates,
        CancellationToken cancellationToken)
    {
        foreach (var hive in new[] { RegistryHive.CurrentUser, RegistryHive.LocalMachine })
        {
            foreach (var view in new[] { RegistryView.Registry64, RegistryView.Registry32 })
            {
                cancellationToken.ThrowIfCancellationRequested();
                try
                {
                    using var baseKey = RegistryKey.OpenBaseKey(hive, view);
                    using var appPaths = baseKey.OpenSubKey(AppPaths);
                    if (appPaths is null)
                    {
                        continue;
                    }
                    foreach (var name in appPaths.GetSubKeyNames())
                    {
                        cancellationToken.ThrowIfCancellationRequested();
                        if (candidates.Count >= MaximumCandidates)
                        {
                            return;
                        }
                        using var application = appPaths.OpenSubKey(name);
                        var path = NormalizeExecutablePath(
                            application?.GetValue(null) as string);
                        if (path is not null)
                        {
                            AddCandidate(candidates, new WindowsApplicationCandidate(
                                path,
                                CleanDisplayName(
                                    Path.GetFileNameWithoutExtension(name),
                                    path),
                                false));
                        }
                    }
                }
                catch (Exception exception) when (
                    exception is IOException
                        or UnauthorizedAccessException
                        or System.Security.SecurityException)
                {
                    // One inaccessible registry view must not hide other applications.
                }
            }
        }
    }

    private static void AddCandidate(
        Dictionary<string, WindowsApplicationCandidate> candidates,
        WindowsApplicationCandidate candidate)
    {
        var key = $"{candidate.Kind}:{candidate.IdentitySource}";
        if (candidates.TryGetValue(key, out var existing))
        {
            candidates[key] = existing with
            {
                IsRunning = existing.IsRunning || candidate.IsRunning,
            };
            return;
        }
        candidates.Add(key, candidate);
    }

    internal static string DisplayNameFor(string path)
    {
        try
        {
            return CleanDisplayName(
                FileVersionInfo.GetVersionInfo(path).FileDescription,
                path);
        }
        catch (Exception exception) when (
            exception is ArgumentException
                or FileNotFoundException
                or IOException
                or NotSupportedException
                or UnauthorizedAccessException
                or System.Security.SecurityException
                or System.ComponentModel.Win32Exception)
        {
            return CleanDisplayName(null, path);
        }
    }

    internal static string? NormalizeExecutablePath(string? value)
    {
        if (string.IsNullOrWhiteSpace(value) || value.Any(char.IsControl))
        {
            return null;
        }
        var expanded = Environment.ExpandEnvironmentVariables(value.Trim());
        if (expanded.Length >= 2 && expanded[0] == '"')
        {
            var closingQuote = expanded.IndexOf('"', 1);
            if (closingQuote <= 1)
            {
                return null;
            }
            expanded = expanded[1..closingQuote];
        }
        try
        {
            if (!Path.IsPathFullyQualified(expanded)
                || !string.Equals(
                    Path.GetExtension(expanded),
                    ".exe",
                    StringComparison.OrdinalIgnoreCase))
            {
                return null;
            }
            var path = Path.GetFullPath(expanded);
            return IsAdapterExecutablePath(path) && File.Exists(path)
                ? path
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

    private static bool IsAdapterExecutablePath(string value)
    {
        return value.Length is >= 7 and <= 4_096
            && char.IsAsciiLetter(value[0])
            && value[1] == ':'
            && value[2] == Path.DirectorySeparatorChar
            && !value.Contains(Path.AltDirectorySeparatorChar)
            && !value[2..].Contains(':')
            && !value.Contains(',')
            && !value.Contains('*')
            && !value.Contains('?')
            && !value.Any(char.IsControl)
            && !value[3..].Split(Path.DirectorySeparatorChar).Any(segment =>
                segment is "" or "." or ".."
                || segment.EndsWith(' ')
                || segment.EndsWith('.'));
    }

    private static string CleanDisplayName(string? value, string path)
    {
        var name = string.IsNullOrWhiteSpace(value) || value.Any(char.IsControl)
            ? Path.GetFileNameWithoutExtension(path)
            : value.Trim();
        return string.IsNullOrWhiteSpace(name) ? "Windows 应用" : name;
    }

    internal static string CleanPackageDisplayName(string? value, string fallback)
    {
        var name = string.IsNullOrWhiteSpace(value) || value.Any(char.IsControl)
            ? fallback
            : value.Trim();
        return string.IsNullOrWhiteSpace(name) || name.Any(char.IsControl)
            ? "Windows 打包应用"
            : name;
    }
}
