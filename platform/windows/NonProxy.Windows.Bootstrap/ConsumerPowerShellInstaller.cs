using System.Diagnostics;
using System.Text.Json;

namespace NonProxy.Windows.Bootstrap;

internal static class ConsumerPowerShellInstaller
{
    public static async Task<BootstrapOperationResult> RunAsync(
        BootstrapAction action,
        ValidatedReleasePackage package)
    {
        var windowsDirectory = Environment.GetFolderPath(
            Environment.SpecialFolder.Windows);
        if (string.IsNullOrWhiteSpace(windowsDirectory))
        {
            throw new InvalidOperationException("无法定位 Windows 系统目录。");
        }
        var executable = Path.GetFullPath(Path.Combine(
            windowsDirectory,
            "System32",
            "WindowsPowerShell",
            "v1.0",
            "powershell.exe"));
        var script = Path.Combine(
            package.PackageRoot,
            "tools",
            "install-system-components.ps1");
        var startInfo = new ProcessStartInfo(executable)
        {
            CreateNoWindow = true,
            RedirectStandardError = true,
            RedirectStandardOutput = true,
            UseShellExecute = false,
        };
        foreach (var argument in new[]
        {
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script,
            "-Action",
            ActionName(action),
            "-PackageRoot",
            package.PackageRoot,
            "-ExpectedPublisherThumbprint",
            package.PublisherThumbprintSha1,
            "-ConsumerBootstrapManifestSha256",
            package.ManifestSha256,
        })
        {
            startInfo.ArgumentList.Add(argument);
        }
        if (action != BootstrapAction.Query)
        {
            startInfo.ArgumentList.Add("-ConfirmSystemMutation");
            startInfo.Environment["NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION"] = "1";
        }
        startInfo.Environment.Remove("NONPROXY_CONFIRM_WINDOWS_DATA_PURGE");
        using var process = Process.Start(startInfo)
            ?? throw new InvalidOperationException("无法启动 Windows 安装事务。");
        var stdoutTask = process.StandardOutput.ReadToEndAsync();
        var stderrTask = process.StandardError.ReadToEndAsync();
        await process.WaitForExitAsync();
        var stdout = await stdoutTask;
        var stderr = await stderrTask;
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException(
                string.IsNullOrWhiteSpace(stderr)
                    ? "Windows 系统组件事务执行失败。"
                    : LastBoundedLine(stderr));
        }
        var payload = LastBoundedLine(stdout);
        using var document = JsonDocument.Parse(payload);
        var requiresReboot = document.RootElement.TryGetProperty(
            "requiresReboot",
            out var reboot)
            && reboot.ValueKind == JsonValueKind.True;
        return new BootstrapOperationResult(payload, requiresReboot);
    }

    private static string ActionName(BootstrapAction action) => action switch
    {
        BootstrapAction.Query => "Query",
        BootstrapAction.Install => "Install",
        BootstrapAction.Repair => "Repair",
        BootstrapAction.Uninstall => "Uninstall",
        _ => throw new ArgumentOutOfRangeException(nameof(action)),
    };

    private static string LastBoundedLine(string value)
    {
        var line = value
            .Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries)
            .LastOrDefault()
            ?.Trim();
        if (string.IsNullOrWhiteSpace(line) || line.Length > 64 * 1024)
        {
            throw new InvalidDataException("Windows 安装事务没有返回有效结果。");
        }
        return line;
    }
}

internal sealed record BootstrapOperationResult(
    string Json,
    bool RequiresReboot);
