namespace NonProxy.Desktop.Windows;

internal sealed record WindowsBootstrapQueryResult(
    int ExitCode,
    string Json);

internal sealed record WindowsBootstrapMutationResult(
    bool Success,
    bool RequiresReboot,
    bool ElevationCancelled,
    int ExitCode);

internal interface IWindowsComponentBootstrap
{
    Task<WindowsBootstrapQueryResult> QueryAsync(
        CancellationToken cancellationToken);

    Task<WindowsBootstrapMutationResult> MutateAsync(
        WindowsBootstrapAction action,
        CancellationToken cancellationToken);
}

internal sealed class WindowsComponentBootstrap(
    IWindowsBootstrapPackageLocator packageLocator,
    IWindowsBootstrapProcessRunner processRunner) : IWindowsComponentBootstrap
{
    private const int RebootRequiredExitCode = 3010;

    public async Task<WindowsBootstrapQueryResult> QueryAsync(
        CancellationToken cancellationToken)
    {
        using var package = packageLocator.Locate();
        var result = await processRunner.RunAsync(
            package,
            WindowsBootstrapAction.Query,
            cancellationToken);
        return new WindowsBootstrapQueryResult(
            result.ExitCode,
            result.StandardOutput
                ?? throw new WindowsBootstrapException(
                    "Windows Bootstrap 未返回状态。",
                    "NP_WINDOWS_BOOTSTRAP_RESULT_INVALID"));
    }

    public async Task<WindowsBootstrapMutationResult> MutateAsync(
        WindowsBootstrapAction action,
        CancellationToken cancellationToken)
    {
        if (action == WindowsBootstrapAction.Query)
        {
            throw new ArgumentException("变更操作不能使用 Query。", nameof(action));
        }
        using var package = packageLocator.Locate();
        var result = await processRunner.RunAsync(
            package,
            action,
            cancellationToken);
        return new WindowsBootstrapMutationResult(
            result.ExitCode is 0 or RebootRequiredExitCode,
            result.ExitCode == RebootRequiredExitCode,
            result.ElevationCancelled,
            result.ExitCode);
    }
}
