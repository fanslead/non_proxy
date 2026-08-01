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
            ["verify"] = Read(root, "verify-release-package.ps1"),
            ["install"] = Read(root, "install-system-components.ps1"),
            ["task"] = Read(root, "NonProxy.Windows.AdapterHost.psm1"),
            ["service"] = Read(root, "NonProxy.Windows.Service.psm1"),
        };
        foreach (var (source, required) in new[]
        {
            ("build", "AdapterHostExecutable"),
            ("build", "nonproxy-adapter-host.exe"),
            ("sign", "adapter/nonproxy-adapter-host.exe"),
            ("verify", "adapter/nonproxy-adapter-host.exe"),
            ("install", "Set-NonProxyAdapterHostTask"),
            ("install", "Remove-NonProxyAdapterHostTask"),
            ("install", "Stop-NonProxyAdapterHostProcesses"),
            ("service", "S-1-5-32-545"),
            ("task", "New-ScheduledTaskTrigger -AtLogOn"),
            ("task", "-RunLevel Limited"),
            ("task", "-MultipleInstances Parallel"),
        })
        {
            Assert.Contains(required, sources[source], StringComparison.Ordinal);
        }
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
