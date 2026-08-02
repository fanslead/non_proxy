namespace NonProxy.Desktop.Tests;

public sealed class WindowsReleaseSourceContractTests
{
    [Fact]
    public void AdapterHostRemainsInPackageTrustAndLifecycleChain()
    {
        var root = FindRepositoryRoot();
        var sources = new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["build"] = Read(root, "build-release-package.ps1"),
            ["sign"] = Read(root, "sign-release-package.ps1"),
            ["developmentSign"] = Read(
                root,
                "sign-development-release-package.ps1"),
            ["developmentCertificate"] = Read(
                root,
                "new-development-signing-certificate.ps1"),
            ["verify"] = Read(root, "verify-release-package.ps1"),
            ["install"] = Read(root, "install-system-components.ps1"),
            ["task"] = Read(root, "NonProxy.Windows.AdapterHost.psm1"),
            ["service"] = Read(root, "NonProxy.Windows.Service.psm1"),
            ["common"] = Read(root, "NonProxy.Windows.Common.psm1"),
        };
        foreach (var (source, required) in new[]
        {
            ("build", "AdapterHostExecutable"),
            ("build", "nonproxy-adapter-host.exe"),
            ("build", "BootstrapPublishDirectory"),
            ("build", "ExpectedPublisherCertificateSha256"),
            ("sign", "adapter/nonproxy-adapter-host.exe"),
            ("sign", "bootstrap/NonProxy.Windows.Bootstrap.exe"),
            ("verify", "adapter/nonproxy-adapter-host.exe"),
            ("verify", "bootstrap/NonProxy.Windows.Bootstrap.exe"),
            ("verify", "ConsumerBootstrapManifestSha256"),
            ("verify", "AllowCrossArchitectureBuildVerification"),
            ("developmentSign", "ExpectedArchitecture"),
            ("developmentSign", "-AllowCrossArchitectureBuildVerification"),
            ("developmentCertificate", "StoreName]::TrustedPeople"),
            ("developmentCertificate", "StoreName]::TrustedPublisher"),
            ("install", "Set-NonProxyAdapterHostTask"),
            ("install", "Remove-NonProxyAdapterHostTask"),
            ("install", "Stop-NonProxyAdapterHostProcesses"),
            ("service", "S-1-5-32-545"),
            ("task", "New-ScheduledTaskTrigger -AtLogOn"),
            ("task", "-RunLevel Limited"),
            ("task", "-MultipleInstances Parallel"),
            ("common", "#requires -Version 5.1"),
        })
        {
            Assert.Contains(required, sources[source], StringComparison.Ordinal);
        }
        Assert.DoesNotContain(
            "StoreName]::Root",
            sources["developmentCertificate"],
            StringComparison.Ordinal);

        var bootstrap = File.ReadAllText(Path.Combine(
            root,
            "platform",
            "windows",
            "NonProxy.Windows.Bootstrap",
            "NonProxy.Windows.Bootstrap.csproj"));
        Assert.Contains("PublishSingleFile", bootstrap, StringComparison.Ordinal);
        Assert.Contains(
            "NonProxyWindowsPublisherCertificateSha256",
            bootstrap,
            StringComparison.Ordinal);
    }

    private static string Read(string root, string fileName) =>
        File.ReadAllText(Path.Combine(root, "scripts", "windows", fileName));

    private static string FindRepositoryRoot()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "Cargo.toml"))
                && Directory.Exists(Path.Combine(
                    directory.FullName,
                    "scripts",
                    "windows")))
            {
                return directory.FullName;
            }
            directory = directory.Parent;
        }
        throw new DirectoryNotFoundException("无法定位 NonProxy Monorepo 根目录。");
    }
}
