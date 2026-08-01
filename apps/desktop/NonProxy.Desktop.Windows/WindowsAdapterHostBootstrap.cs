using System.Diagnostics;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.Win32;

namespace NonProxy.Desktop.Windows;

internal static class WindowsAdapterHostBootstrap
{
    private const string RegistryPath = @"Software\NonProxy\System";
    private const string ExecutableName = "nonproxy-adapter-host.exe";
    private const int MaximumRuntimeIdentityBytes = 4 * 1024;
    private const long MaximumExecutableBytes = 128L * 1024 * 1024;

    public static WindowsAdapterHostStartStatus TryStart()
    {
        if (!OperatingSystem.IsWindows())
        {
            return WindowsAdapterHostStartStatus.UnsupportedPlatform;
        }

        try
        {
            using var localMachine = RegistryKey.OpenBaseKey(
                RegistryHive.LocalMachine,
                RegistryView.Registry64);
            using var metadata = localMachine.OpenSubKey(RegistryPath);
            var installRoot = metadata?.GetValue("InstallRoot") as string;
            var fingerprint = metadata?.GetValue("AdapterHostFingerprint") as string;
            var programFiles = Environment.GetFolderPath(
                Environment.SpecialFolder.ProgramFiles);
            if (string.IsNullOrWhiteSpace(installRoot)
                || string.IsNullOrWhiteSpace(fingerprint)
                || string.IsNullOrWhiteSpace(programFiles))
            {
                return WindowsAdapterHostStartStatus.NotInstalled;
            }

            var programRoot = Path.Combine(programFiles, "NonProxy", "system");
            var executable = ResolveTrustedExecutable(
                programRoot,
                installRoot,
                fingerprint);
            if (IsInstalledHostRunning(programRoot))
            {
                return WindowsAdapterHostStartStatus.AlreadyRunning;
            }

            var startInfo = new ProcessStartInfo(executable)
            {
                CreateNoWindow = true,
                UseShellExecute = false,
                WorkingDirectory = Path.GetDirectoryName(executable)!,
            };
            foreach (var name in new[]
            {
                "NONPROXY_ADAPTER_STATE_DIR",
                "NONPROXY_ADAPTER_SOCKET_PATH",
                "NONPROXY_ADAPTER_BUNDLE_FINGERPRINT",
                "NONPROXY_WINDOWS_ADAPTER_PIPE",
                "NONPROXY_WINDOWS_ADAPTER_PIPE_SDDL",
            })
            {
                startInfo.Environment.Remove(name);
            }

            using var process = Process.Start(startInfo);
            return process is null
                ? WindowsAdapterHostStartStatus.StartFailed
                : WindowsAdapterHostStartStatus.Started;
        }
        catch (Exception exception) when (
            exception is IOException
                or UnauthorizedAccessException
                or InvalidOperationException
                or ArgumentException
                or CryptographicException
                or JsonException
                or System.ComponentModel.Win32Exception
                or System.Security.SecurityException)
        {
            return WindowsAdapterHostStartStatus.Rejected;
        }
    }

    internal static string ResolveTrustedExecutable(
        string programRoot,
        string installRoot,
        string fingerprint)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(programRoot);
        ArgumentException.ThrowIfNullOrWhiteSpace(installRoot);
        if (!IsCanonicalSha256(fingerprint))
        {
            throw new ArgumentException(
                "Adapter Host 指纹格式无效。",
                nameof(fingerprint));
        }

        var trustedRoot = Path.GetFullPath(programRoot)
            .TrimEnd(Path.DirectorySeparatorChar);
        var versionRoot = Path.GetFullPath(installRoot)
            .TrimEnd(Path.DirectorySeparatorChar);
        if (!string.Equals(
                Path.GetDirectoryName(versionRoot),
                trustedRoot,
                StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "Adapter Host 安装目录不属于受保护的版本根目录。");
        }

        var executable = Path.Combine(
            versionRoot,
            "adapter",
            ExecutableName);
        AssertRegularPath(trustedRoot);
        AssertRegularPath(versionRoot);
        AssertRegularPath(Path.GetDirectoryName(executable)!);
        AssertRegularPath(executable);
        var actual = HashExecutable(executable);
        if (!string.Equals(actual, fingerprint, StringComparison.Ordinal))
        {
            throw new CryptographicException("Adapter Host 文件哈希不匹配。");
        }
        return executable;
    }

    internal static WindowsAdapterRuntimeIdentity? TryReadRuntimeIdentity(
        string path)
    {
        try
        {
            AssertRegularPath(path);
            var file = new FileInfo(path);
            if (!file.Exists || file.Length is <= 0 or > MaximumRuntimeIdentityBytes)
            {
                return null;
            }
            using var stream = file.OpenRead();
            var identity = JsonSerializer.Deserialize<WindowsAdapterRuntimeIdentity>(
                stream);
            return identity is
            {
                SchemaVersion: 1,
                ProcessId: > 0,
            } && IsCanonicalSha256(identity.BundleFingerprint)
                ? identity
                : null;
        }
        catch (Exception exception) when (
            exception is IOException
                or UnauthorizedAccessException
                or JsonException
                or ArgumentException)
        {
            return null;
        }
    }

    private static bool IsInstalledHostRunning(string programRoot)
    {
        var localApplicationData = Environment.GetFolderPath(
            Environment.SpecialFolder.LocalApplicationData);
        if (string.IsNullOrWhiteSpace(localApplicationData))
        {
            return false;
        }
        var identity = TryReadRuntimeIdentity(Path.Combine(
            localApplicationData,
            "NonProxy",
            "adapter-host",
            "adapter.runtime.json"));
        if (identity is null)
        {
            return false;
        }

        try
        {
            using var process = Process.GetProcessById(identity.ProcessId);
            var executable = process.MainModule?.FileName;
            if (process.HasExited || string.IsNullOrWhiteSpace(executable))
            {
                return false;
            }
            var versionRoot = Directory.GetParent(
                Path.GetDirectoryName(executable)!)?.FullName;
            if (string.IsNullOrWhiteSpace(versionRoot))
            {
                return false;
            }
            return string.Equals(
                ResolveTrustedExecutable(
                    programRoot,
                    versionRoot,
                    identity.BundleFingerprint),
                Path.GetFullPath(executable),
                StringComparison.OrdinalIgnoreCase);
        }
        catch (Exception exception) when (
            exception is ArgumentException
                or InvalidOperationException
                or IOException
                or UnauthorizedAccessException
                or CryptographicException
                or System.ComponentModel.Win32Exception)
        {
            return false;
        }
    }

    private static void AssertRegularPath(string path)
    {
        var attributes = File.GetAttributes(path);
        if ((attributes & FileAttributes.ReparsePoint) != 0)
        {
            throw new InvalidOperationException("Adapter Host 路径不允许重解析点。");
        }
    }

    private static string HashExecutable(string path)
    {
        using var stream = new FileStream(
            path,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read);
        var expectedLength = stream.Length;
        if (expectedLength is <= 0 or > MaximumExecutableBytes)
        {
            throw new InvalidOperationException("Adapter Host 文件大小无效。");
        }
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var buffer = new byte[64 * 1024];
        long consumed = 0;
        while (consumed <= MaximumExecutableBytes)
        {
            var read = stream.Read(buffer);
            if (read == 0)
            {
                break;
            }
            consumed = checked(consumed + read);
            hash.AppendData(buffer, 0, read);
        }
        if (consumed != expectedLength || consumed > MaximumExecutableBytes)
        {
            throw new InvalidOperationException("Adapter Host 文件在校验期间发生变化。");
        }
        return Convert.ToHexString(hash.GetHashAndReset()).ToLowerInvariant();
    }

    private static bool IsCanonicalSha256(string? value) =>
        value is { Length: 64 }
        && value.All(character =>
            char.IsAsciiDigit(character) || character is >= 'a' and <= 'f');
}

internal enum WindowsAdapterHostStartStatus
{
    UnsupportedPlatform,
    NotInstalled,
    AlreadyRunning,
    Started,
    StartFailed,
    Rejected,
}

internal sealed record WindowsAdapterRuntimeIdentity(
    [property: JsonPropertyName("schemaVersion")] int SchemaVersion,
    [property: JsonPropertyName("bundleFingerprint")] string BundleFingerprint,
    [property: JsonPropertyName("processId")] int ProcessId);
