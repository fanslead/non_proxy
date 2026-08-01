using System.Text.Json;
using System.Text.Json.Serialization;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Windows;

internal sealed class WindowsSystemComponentInstaller(
    IWindowsComponentBootstrap bootstrap) : ISystemComponentInstaller
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        MaxDepth = 8,
        PropertyNameCaseInsensitive = false,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
    };

    public async Task<SystemComponentState> GetStateAsync(
        CancellationToken cancellationToken)
    {
        try
        {
            var result = await bootstrap.QueryAsync(cancellationToken);
            var payload = JsonSerializer.Deserialize<WindowsComponentPayload>(
                result.Json,
                JsonOptions)
                ?? throw new JsonException("Windows Bootstrap 状态为空。");
            return MapState(payload);
        }
        catch (WindowsBootstrapException exception)
        {
            return new SystemComponentState(
                SystemComponentStatus.Unavailable,
                exception.Message,
                exception.ErrorCode);
        }
        catch (Exception exception) when (exception is JsonException or IOException)
        {
            return new SystemComponentState(
                SystemComponentStatus.Failed,
                "Windows 系统组件状态响应无效。",
                "NP_WINDOWS_COMPONENT_STATE_INVALID");
        }
    }

    public async Task<InstallResult> InstallAsync(
        CancellationToken cancellationToken)
    {
        return await MutateAsync(
            WindowsBootstrapAction.Install,
            "Windows 系统组件安装事务已完成。",
            cancellationToken);
    }

    public async Task<InstallResult> UninstallAsync(
        CancellationToken cancellationToken)
    {
        return await MutateAsync(
            WindowsBootstrapAction.Uninstall,
            "Windows 系统组件已卸载；用户规则和本地配置默认保留。",
            cancellationToken);
    }

    public Task<InstallResult> OpenSystemSettingsAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(InstallResult.Unavailable(
            "Windows 系统组件通过安装时的 UAC 对话框授权，没有单独的系统设置页。",
            "NP_WINDOWS_AUTHORIZATION_PAGE_NOT_APPLICABLE"));
    }

    private async Task<InstallResult> MutateAsync(
        WindowsBootstrapAction action,
        string successMessage,
        CancellationToken cancellationToken)
    {
        try
        {
            var result = await bootstrap.MutateAsync(action, cancellationToken);
            if (result.ElevationCancelled)
            {
                return new InstallResult(
                    false,
                    "已取消 Windows 管理员授权，系统组件未变更。",
                    "NP_WINDOWS_ELEVATION_CANCELLED");
            }
            if (!result.Success)
            {
                return new InstallResult(
                    false,
                    "Windows 系统组件事务失败，请重新检查组件状态。",
                    "NP_WINDOWS_COMPONENT_TRANSACTION_FAILED");
            }
            if (action != WindowsBootstrapAction.Uninstall)
            {
                _ = WindowsAdapterHostBootstrap.TryStart();
            }
            return new InstallResult(
                true,
                result.RequiresReboot
                    ? "系统组件变更已提交，需要由你选择时间重启 Windows。"
                    : successMessage,
                RequiresReboot: result.RequiresReboot);
        }
        catch (WindowsBootstrapException exception)
        {
            return new InstallResult(false, exception.Message, exception.ErrorCode);
        }
    }

    private static SystemComponentState MapState(WindowsComponentPayload payload)
    {
        var status = payload.Status switch
        {
            "Installed" => SystemComponentStatus.Installed,
            "NotInstalled" => SystemComponentStatus.NotInstalled,
            "Partial" => SystemComponentStatus.Failed,
            "Unavailable" => SystemComponentStatus.Unavailable,
            _ => SystemComponentStatus.Unknown,
        };
        return new SystemComponentState(
            status,
            string.IsNullOrWhiteSpace(payload.Message)
                ? "Windows 系统组件未返回说明。"
                : payload.Message,
            payload.ErrorCode,
            payload.Steps?.Select(MapStep).ToArray());
    }

    private static SystemComponentStep MapStep(WindowsComponentStepPayload step)
    {
        var status = step.Installed
            ? step.Status switch
            {
                "Ready" or "Running" => SystemComponentStepStatus.Ready,
                _ => SystemComponentStepStatus.NeedsRepair,
            }
            : SystemComponentStepStatus.NotInstalled;
        return new SystemComponentStep(
            step.Id ?? "unknown",
            step.Name ?? "未知组件",
            status,
            step.Status ?? "未返回状态。");
    }

    private sealed class WindowsComponentPayload
    {
        public bool Success { get; init; }

        public string? Status { get; init; }

        public string? Message { get; init; }

        public string? ErrorCode { get; init; }

        public bool RequiresReboot { get; init; }

        public WindowsComponentStepPayload[]? Steps { get; init; }
    }

    private sealed class WindowsComponentStepPayload
    {
        public string? Id { get; init; }

        public string? Name { get; init; }

        public bool Installed { get; init; }

        public string? Status { get; init; }
    }
}
