using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using NonProxy.Windows.Bootstrap;
using NonProxy.Windows.Security;

namespace NonProxy.Desktop.Tests;

public sealed class WindowsBootstrapPackageValidatorTests
{
    private const string Publisher =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    [Fact]
    public void ArgumentsRequireExactActionAndPackageRootFlag()
    {
        var parsed = BootstrapArguments.Parse(
            ["repair", "--package-root", "."]);

        Assert.Equal(BootstrapAction.Repair, parsed.Action);
        Assert.Equal(Path.GetFullPath("."), parsed.PackageRoot);
        Assert.Throws<ArgumentException>(() =>
            BootstrapArguments.Parse(["install", "--root", "."]));
    }

    [Fact]
    public void ValidatesSignedManifestFilesAndExplicitDriverCatalogMembers()
    {
        using var package = TestReleasePackage.Create();
        var trust = new RecordingTrustVerifier();

        var result = new ReleasePackageValidator(trust, Publisher)
            .Validate(package.Path);

        Assert.Equal(package.ManifestHash, result.ManifestSha256);
        Assert.Equal(Publisher, result.PublisherCertificateSha256);
        Assert.Equal(2, trust.CatalogMembers.Count);
        Assert.Contains(
            trust.CatalogMembers,
            member => member.EndsWith("NonProxyWfp.inf", StringComparison.Ordinal));
        Assert.Contains(
            trust.CatalogMembers,
            member => member.EndsWith("NonProxyWfp.sys", StringComparison.Ordinal));
    }

    [Fact]
    public void RejectsManifestMismatchAndFilesOutsideTheSignedInventory()
    {
        using var package = TestReleasePackage.Create();
        var validator = new ReleasePackageValidator(
            new RecordingTrustVerifier(),
            Publisher);
        File.WriteAllText(Path.Combine(package.Path, "extra.txt"), "unexpected");

        Assert.Throws<InvalidDataException>(() => validator.Validate(package.Path));

        File.Delete(Path.Combine(package.Path, "extra.txt"));
        File.AppendAllText(
            Path.Combine(package.Path, "release-manifest.json"),
            " ");
        Assert.Throws<CryptographicException>(() =>
            validator.Validate(package.Path));
    }

    private sealed class RecordingTrustVerifier : IReleaseTrustVerifier
    {
        public List<string> CatalogMembers { get; } = [];

        public WindowsSignerCertificate? VerifyAuthenticode(string path)
        {
            return new WindowsSignerCertificate(
                new string('B', 40),
                Publisher);
        }

        public void VerifyCatalogMember(string catalogPath, string memberPath)
        {
            Assert.EndsWith(
                "NonProxyWfp.cat",
                catalogPath,
                StringComparison.Ordinal);
            CatalogMembers.Add(memberPath);
        }
    }

    private sealed class TestReleasePackage : IDisposable
    {
        private static readonly string[] Files =
        [
            "adapter/nonproxy-adapter-host.exe",
            "bootstrap/NonProxy.Windows.Bootstrap.exe",
            "desktop/NonProxy.Desktop.Windows.exe",
            "driver/NonProxyWfp.cat",
            "driver/NonProxyWfp.inf",
            "driver/NonProxyWfp.sys",
            "release-metadata.json",
            "service/nonproxy-gatewayd.exe",
            "tools/install-system-components.ps1",
            "tools/NonProxy.Windows.AdapterHost.psm1",
            "tools/NonProxy.Windows.Common.psm1",
            "tools/NonProxy.Windows.DriverPackage.psm1",
            "tools/NonProxy.Windows.Service.psm1",
            "tools/verify-release-package.ps1",
        ];
        private readonly DirectoryInfo _directory =
            Directory.CreateTempSubdirectory("nonproxy-windows-release-");

        public string Path => _directory.FullName;

        public string ManifestHash { get; private set; } = string.Empty;

        public static TestReleasePackage Create()
        {
            var package = new TestReleasePackage();
            package.Write();
            return package;
        }

        public void Dispose()
        {
            _directory.Delete(recursive: true);
        }

        private void Write()
        {
            foreach (var relative in Files)
            {
                var path = System.IO.Path.Combine(
                    Path,
                    relative.Replace('/', System.IO.Path.DirectorySeparatorChar));
                Directory.CreateDirectory(System.IO.Path.GetDirectoryName(path)!);
                File.WriteAllText(path, $"fixture:{relative}");
            }
            var entries = Files.Select(relative =>
            {
                var path = System.IO.Path.Combine(
                    Path,
                    relative.Replace('/', System.IO.Path.DirectorySeparatorChar));
                var bytes = File.ReadAllBytes(path);
                return new
                {
                    path = relative,
                    size = bytes.LongLength,
                    sha256 = Convert.ToHexString(SHA256.HashData(bytes))
                        .ToLowerInvariant(),
                };
            }).ToArray();
            var architecture = RuntimeInformation.OSArchitecture switch
            {
                Architecture.X64 => "x64",
                Architecture.Arm64 => "arm64",
                _ => throw new PlatformNotSupportedException(),
            };
            var manifest = JsonSerializer.Serialize(new
            {
                schemaVersion = 1,
                product = "NonProxy",
                version = "0.1.0",
                architecture,
                minimumWindowsBuild = 18362,
                publisherCertificateSha256 = Publisher,
                publisherThumbprintHint = new string('B', 40),
                signedUtc = DateTimeOffset.UtcNow.ToString("o"),
                files = entries,
            });
            var manifestPath = System.IO.Path.Combine(
                Path,
                "release-manifest.json");
            File.WriteAllText(manifestPath, manifest);
            ManifestHash = Convert.ToHexString(
                    SHA256.HashData(File.ReadAllBytes(manifestPath)))
                .ToLowerInvariant();
            File.WriteAllText(
                System.IO.Path.Combine(Path, "release-trust.ps1"),
                $"$NonProxyReleaseManifestSha256 = '{ManifestHash}'\n" +
                "# SIG # Begin signature block\nfixture");
        }
    }
}
