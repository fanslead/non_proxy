using System.Security.Cryptography;
using NonProxy.Desktop.Windows;

namespace NonProxy.Desktop.Tests;

public sealed class WindowsAdapterHostBootstrapTests
{
    [Fact]
    public void ResolvesOnlyHashedAdapterInsideOneVersionDirectory()
    {
        using var directory = new TemporaryDirectory();
        var programRoot = Path.Combine(directory.Path, "NonProxy", "system");
        var installRoot = Path.Combine(programRoot, "0.1.0-x64-0123456789ab");
        var adapterDirectory = Path.Combine(installRoot, "adapter");
        Directory.CreateDirectory(adapterDirectory);
        var executable = Path.Combine(
            adapterDirectory,
            "nonproxy-adapter-host.exe");
        File.WriteAllBytes(executable, "signed-adapter-host"u8.ToArray());
        var fingerprint = Convert.ToHexString(
                SHA256.HashData("signed-adapter-host"u8))
            .ToLowerInvariant();

        var resolved = WindowsAdapterHostBootstrap.ResolveTrustedExecutable(
            programRoot,
            installRoot,
            fingerprint);

        Assert.Equal(executable, resolved);
    }

    [Fact]
    public void RejectsOutsideInstallRootAndChangedExecutable()
    {
        using var directory = new TemporaryDirectory();
        var programRoot = Path.Combine(directory.Path, "NonProxy", "system");
        var outside = Path.Combine(directory.Path, "outside");
        var adapterDirectory = Path.Combine(outside, "adapter");
        Directory.CreateDirectory(adapterDirectory);
        File.WriteAllBytes(
            Path.Combine(adapterDirectory, "nonproxy-adapter-host.exe"),
            "changed"u8.ToArray());

        Assert.Throws<InvalidOperationException>(() =>
            WindowsAdapterHostBootstrap.ResolveTrustedExecutable(
                programRoot,
                outside,
                new string('a', 64)));

        var installRoot = Path.Combine(programRoot, "0.1.0-x64-0123456789ab");
        adapterDirectory = Path.Combine(installRoot, "adapter");
        Directory.CreateDirectory(adapterDirectory);
        File.WriteAllBytes(
            Path.Combine(adapterDirectory, "nonproxy-adapter-host.exe"),
            "changed"u8.ToArray());
        Assert.Throws<CryptographicException>(() =>
            WindowsAdapterHostBootstrap.ResolveTrustedExecutable(
                programRoot,
                installRoot,
                new string('a', 64)));
    }

    [Fact]
    public void RuntimeIdentityIsBoundedAndRequiresCanonicalFingerprint()
    {
        using var directory = new TemporaryDirectory();
        var valid = Path.Combine(directory.Path, "valid.json");
        File.WriteAllText(
            valid,
            $$"""
            {
              "schemaVersion": 1,
              "bundleFingerprint": "{{new string('a', 64)}}",
              "processId": 42,
              "semanticVersion": "0.1.0",
              "buildId": "release"
            }
            """);
        var invalid = Path.Combine(directory.Path, "invalid.json");
        File.WriteAllText(
            invalid,
            $$"""{"schemaVersion":1,"bundleFingerprint":"{{new string('A', 64)}}","processId":42}""");
        var oversized = Path.Combine(directory.Path, "oversized.json");
        File.WriteAllText(oversized, new string('x', 4097));

        var identity = WindowsAdapterHostBootstrap.TryReadRuntimeIdentity(valid);

        Assert.NotNull(identity);
        Assert.Equal(42, identity.ProcessId);
        Assert.Null(WindowsAdapterHostBootstrap.TryReadRuntimeIdentity(invalid));
        Assert.Null(WindowsAdapterHostBootstrap.TryReadRuntimeIdentity(oversized));
    }

    private sealed class TemporaryDirectory : IDisposable
    {
        private readonly DirectoryInfo _directory =
            Directory.CreateTempSubdirectory("nonproxy-windows-adapter-");

        public string Path => _directory.FullName;

        public void Dispose()
        {
            _directory.Delete(recursive: true);
        }
    }
}
