using System.Diagnostics;
using System.Runtime.Versioning;
using Microsoft.Win32;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

[SupportedOSPlatform("windows")]
internal sealed class WindowsApplicationDiscovery : IWindowsApplicationDiscovery
{
    private const int MaximumCandidates = 1024;
    private const string AppPaths =
        @"Software\Microsoft\Windows\CurrentVersion\App Paths";

    public IReadOnlyList<WindowsApplicationCandidate> Discover(
        CancellationToken cancellationToken)
    {
        var candidates = new Dictionary<string, WindowsApplicationCandidate>(
            StringComparer.OrdinalIgnoreCase);
        AddRunning(candidates, cancellationToken);
        AddRegistered(candidates, cancellationToken);
        return candidates.Values.ToArray();
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
                        AddCandidate(
                            candidates,
                            path,
                            CleanDisplayName(process.ProcessName, path),
                            true);
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
                            AddCandidate(
                                candidates,
                                path,
                                CleanDisplayName(
                                    Path.GetFileNameWithoutExtension(name),
                                    path),
                                false);
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
        string path,
        string displayName,
        bool isRunning)
    {
        if (candidates.TryGetValue(path, out var existing))
        {
            candidates[path] = existing with
            {
                IsRunning = existing.IsRunning || isRunning,
            };
            return;
        }
        candidates.Add(path, new WindowsApplicationCandidate(
            path,
            displayName,
            isRunning));
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
            return File.Exists(path) ? path : null;
        }
        catch (Exception exception) when (
            exception is ArgumentException
                or NotSupportedException
                or PathTooLongException)
        {
            return null;
        }
    }

    private static string CleanDisplayName(string? value, string path)
    {
        var name = string.IsNullOrWhiteSpace(value) || value.Any(char.IsControl)
            ? Path.GetFileNameWithoutExtension(path)
            : value.Trim();
        return string.IsNullOrWhiteSpace(name) ? "Windows 应用" : name;
    }
}
