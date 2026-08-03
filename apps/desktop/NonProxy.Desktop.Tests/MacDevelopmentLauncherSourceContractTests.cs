using System.Diagnostics;

namespace NonProxy.Desktop.Tests;

public sealed class MacDevelopmentLauncherSourceContractTests
{
    [Fact]
    public void LauncherKeepsUserSpaceDevelopmentSeparateFromSystemTakeover()
    {
        var root = FindRepositoryRoot();
        var launcher = File.ReadAllText(Path.Combine(
            root,
            "scripts",
            "macos",
            "run-development.sh"));
        var readme = File.ReadAllText(Path.Combine(root, "README.md"));

        foreach (var required in new[]
        {
            "NONPROXY_STATE_DIR",
            "NONPROXY_ADAPTER_STATE_DIR",
            "nonproxy-gatewayd",
            "nonproxy-adapter-host",
            "NonProxy.Desktop.Mac",
            "--smoke",
            "unix_socket_path_max_bytes=103",
            "wc -c",
            "不会接管系统流量",
        })
        {
            Assert.Contains(required, launcher, StringComparison.Ordinal);
        }

        Assert.DoesNotContain(
            "NONPROXY_ALLOW_SYSTEM_MUTATION=1",
            launcher,
            StringComparison.Ordinal);
        Assert.DoesNotContain(
            "systemextensionsctl",
            launcher,
            StringComparison.Ordinal);
        Assert.Contains(
            "./scripts/macos/run-development.sh",
            readme,
            StringComparison.Ordinal);
        Assert.Contains(
            "不会捕获或改写本机真实流量",
            readme,
            StringComparison.Ordinal);
    }

    [Fact]
    public void LauncherRejectsOverlongMacSocketPathBeforeBuilding()
    {
        if (!OperatingSystem.IsMacOS())
        {
            return;
        }

        var root = FindRepositoryRoot();
        var launcher = Path.Combine(root, "scripts", "macos", "run-development.sh");
        var startInfo = new ProcessStartInfo(launcher)
        {
            RedirectStandardError = true,
            RedirectStandardOutput = true,
            UseShellExecute = false,
        };
        startInfo.ArgumentList.Add("--smoke");
        startInfo.ArgumentList.Add("--state-directory");
        startInfo.ArgumentList.Add(Path.Combine("/tmp", new string('x', 90)));

        using var process = Process.Start(startInfo);
        Assert.NotNull(process);
        var standardOutput = process.StandardOutput.ReadToEnd();
        var standardError = process.StandardError.ReadToEnd();
        Assert.True(process.WaitForExit(5_000), "开发启动器的路径预检没有及时退出。");
        Assert.Equal(64, process.ExitCode);
        Assert.Contains("103 字节上限", standardError, StringComparison.Ordinal);
        Assert.DoesNotContain(
            "Determining projects to restore",
            standardOutput,
            StringComparison.Ordinal);
    }

    private static string FindRepositoryRoot()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "Cargo.toml"))
                && Directory.Exists(Path.Combine(
                    directory.FullName,
                    "scripts",
                    "macos")))
            {
                return directory.FullName;
            }
            directory = directory.Parent;
        }
        throw new DirectoryNotFoundException("无法定位 NonProxy Monorepo 根目录。");
    }
}
