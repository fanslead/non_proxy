using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.Versioning;

namespace NonProxy.Desktop.Windows;

internal enum WindowsBootstrapAction
{
    Query,
    Install,
    Repair,
    Uninstall,
}

internal sealed record WindowsBootstrapProcessResult(
    int ExitCode,
    string? StandardOutput,
    bool ElevationCancelled = false);

internal interface IWindowsBootstrapProcessRunner
{
    Task<WindowsBootstrapProcessResult> RunAsync(
        WindowsBootstrapPackage package,
        WindowsBootstrapAction action,
        CancellationToken cancellationToken);
}

[SupportedOSPlatform("windows")]
internal sealed class WindowsBootstrapProcessRunner :
    IWindowsBootstrapProcessRunner
{
    private static readonly TimeSpan QueryTimeout = TimeSpan.FromMinutes(2);

    public async Task<WindowsBootstrapProcessResult> RunAsync(
        WindowsBootstrapPackage package,
        WindowsBootstrapAction action,
        CancellationToken cancellationToken)
    {
        var mutation = action != WindowsBootstrapAction.Query;
        var startInfo = new ProcessStartInfo(package.BootstrapExecutable)
        {
            UseShellExecute = mutation,
            Verb = mutation ? "runas" : string.Empty,
            CreateNoWindow = !mutation,
            RedirectStandardOutput = !mutation,
            RedirectStandardError = !mutation,
            WorkingDirectory = package.PackageRoot,
        };
        startInfo.ArgumentList.Add(ActionName(action));
        startInfo.ArgumentList.Add("--package-root");
        startInfo.ArgumentList.Add(package.PackageRoot);
        try
        {
            using var process = Process.Start(startInfo)
                ?? throw new WindowsBootstrapException(
                    "无法启动 Windows 安装 Bootstrap。",
                    "NP_WINDOWS_BOOTSTRAP_START_FAILED");
            if (mutation)
            {
                await process.WaitForExitAsync(cancellationToken);
                return new WindowsBootstrapProcessResult(process.ExitCode, null);
            }
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(
                cancellationToken);
            timeout.CancelAfter(QueryTimeout);
            var stdoutTask = process.StandardOutput.ReadToEndAsync(timeout.Token);
            var stderrTask = process.StandardError.ReadToEndAsync(timeout.Token);
            try
            {
                await process.WaitForExitAsync(timeout.Token);
            }
            catch (OperationCanceledException) when (
                !cancellationToken.IsCancellationRequested)
            {
                process.Kill(entireProcessTree: true);
                throw new WindowsBootstrapException(
                    "Windows 组件状态查询超时。",
                    "NP_WINDOWS_BOOTSTRAP_TIMEOUT");
            }
            var stdout = await stdoutTask;
            var stderr = await stderrTask;
            if (string.IsNullOrWhiteSpace(stdout) && !string.IsNullOrWhiteSpace(stderr))
            {
                throw new WindowsBootstrapException(
                    LastBoundedLine(stderr),
                    "NP_WINDOWS_BOOTSTRAP_QUERY_FAILED");
            }
            return new WindowsBootstrapProcessResult(
                process.ExitCode,
                LastBoundedLine(stdout));
        }
        catch (Win32Exception exception) when (
            mutation && exception.NativeErrorCode == 1223)
        {
            return new WindowsBootstrapProcessResult(
                exception.NativeErrorCode,
                null,
                ElevationCancelled: true);
        }
        catch (Win32Exception exception)
        {
            throw new WindowsBootstrapException(
                "Windows 无法启动安装 Bootstrap。",
                "NP_WINDOWS_BOOTSTRAP_START_FAILED",
                exception);
        }
    }

    private static string ActionName(WindowsBootstrapAction action) =>
        action.ToString().ToLowerInvariant();

    private static string LastBoundedLine(string value)
    {
        var line = value
            .Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries)
            .LastOrDefault()
            ?.Trim();
        if (string.IsNullOrWhiteSpace(line) || line.Length > 256 * 1024)
        {
            throw new WindowsBootstrapException(
                "Windows 安装 Bootstrap 没有返回有效结果。",
                "NP_WINDOWS_BOOTSTRAP_RESULT_INVALID");
        }
        return line;
    }
}
