using System.Collections.Concurrent;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

internal sealed record WindowsApplicationCandidate(
    string ExecutablePath,
    string DisplayName,
    bool IsRunning);

internal interface IWindowsApplicationDiscovery
{
    IReadOnlyList<WindowsApplicationCandidate> Discover(
        CancellationToken cancellationToken);
}

internal interface IWindowsApplicationIdentityReader
{
    ApplicationCatalogEntry? Read(WindowsApplicationCandidate candidate);
}

internal interface IWindowsExecutablePicker
{
    Task<string?> PickAsync(CancellationToken cancellationToken);
}

internal sealed class WindowsApplicationCatalog(
    IWindowsApplicationDiscovery discovery,
    IWindowsApplicationIdentityReader identityReader,
    IWindowsExecutablePicker picker) : IApplicationCatalog
{
    public async Task<ApplicationCatalogSnapshot> ListAsync(
        CancellationToken cancellationToken)
    {
        try
        {
            var candidates = await Task.Run(
                () => discovery.Discover(cancellationToken),
                cancellationToken);
            var applications = await Task.Run(
                () => Resolve(candidates, cancellationToken),
                cancellationToken);
            var skipped = candidates.Count - applications.Length;
            var message = skipped == 0
                ? $"已找到 {applications.Length} 个可信应用。"
                : $"已找到 {applications.Length} 个可信应用；另有 {skipped} 个项目因路径或签名身份不足未显示。";
            return new ApplicationCatalogSnapshot(
                applications,
                true,
                true,
                message);
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (Exception exception) when (IsExpectedCatalogFailure(exception))
        {
            return ApplicationCatalogSnapshot.Unavailable(
                "Windows 应用目录暂时不可用；已有应用规则仍可正常查看和删除。");
        }
    }

    public async Task<ApplicationSelectionResult> ChooseAsync(
        CancellationToken cancellationToken)
    {
        try
        {
            var path = await picker.PickAsync(cancellationToken);
            if (path is null)
            {
                return new ApplicationSelectionResult(false, null, "未选择应用，规则没有变化。");
            }

            var candidate = new WindowsApplicationCandidate(
                path,
                Path.GetFileNameWithoutExtension(path),
                false);
            var application = await Task.Run(
                () => identityReader.Read(candidate),
                cancellationToken);
            return application is null
                ? new ApplicationSelectionResult(
                    false,
                    null,
                    "所选文件没有可验证的 Windows 应用身份或可信 Authenticode 签名；未创建规则。")
                : new ApplicationSelectionResult(
                    true,
                    application,
                    $"已验证“{application.DisplayName}”的 Windows 应用身份。继续保存后，规则将等待系统组件确认。");
        }
        catch (Exception exception) when (IsExpectedCatalogFailure(exception))
        {
            return new ApplicationSelectionResult(
                false,
                null,
                "Windows 无法验证所选应用的系统身份；未创建规则。请重试或查看诊断状态。");
        }
    }

    private ApplicationCatalogEntry[] Resolve(
        IReadOnlyList<WindowsApplicationCandidate> candidates,
        CancellationToken cancellationToken)
    {
        var applications = new ConcurrentDictionary<string, ApplicationCatalogEntry>(
            StringComparer.Ordinal);
        Parallel.ForEach(
            candidates,
            new ParallelOptions
            {
                CancellationToken = cancellationToken,
                MaxDegreeOfParallelism = Math.Clamp(
                    Environment.ProcessorCount,
                    1,
                    4),
            },
            candidate =>
        {
            var application = identityReader.Read(candidate);
            if (application is null)
            {
                return;
            }
            applications.AddOrUpdate(
                application.StableIdentity,
                application,
                (_, existing) => existing.IsRunning || !application.IsRunning
                    ? existing
                    : application);
        });
        return applications.Values
            .OrderByDescending(application => application.IsRunning)
            .ThenBy(application => application.DisplayName)
            .ToArray();
    }

    private static bool IsExpectedCatalogFailure(Exception exception)
    {
        return exception is AggregateException aggregate
            ? aggregate.InnerExceptions.All(IsExpectedCatalogFailure)
            : exception is InvalidOperationException
            or ArgumentException
            or IOException
            or NotSupportedException
            or UnauthorizedAccessException
            or System.ComponentModel.Win32Exception
            or System.Security.SecurityException
            or DllNotFoundException
            or EntryPointNotFoundException
            or BadImageFormatException;
    }
}
