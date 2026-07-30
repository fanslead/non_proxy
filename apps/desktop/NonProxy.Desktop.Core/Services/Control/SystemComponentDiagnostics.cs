using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Services.Control;

internal static class SystemComponentDiagnostics
{
    internal static void AddTo(
        ICollection<DiagnosticCheck> checks,
        SystemComponentState component)
    {
        ArgumentNullException.ThrowIfNull(checks);
        ArgumentNullException.ThrowIfNull(component);

        checks.Add(new DiagnosticCheck(
            "system-component",
            "系统网络组件",
            StatusLabel(component.Status),
            ComponentDetail(component)));
        foreach (var step in component.Steps)
        {
            checks.Add(new DiagnosticCheck(
                $"system-component-{step.Id}",
                step.Name,
                step.StatusLabel,
                step.Detail));
        }
    }

    private static string ComponentDetail(SystemComponentState component)
    {
        return string.IsNullOrWhiteSpace(component.ErrorCode)
            ? component.Message
            : $"{component.Message}（错误码：{component.ErrorCode}）";
    }

    private static string StatusLabel(SystemComponentStatus status)
    {
        return status switch
        {
            SystemComponentStatus.Installed => "已就绪",
            SystemComponentStatus.AwaitingApproval => "等待授权",
            SystemComponentStatus.NotInstalled => "未安装",
            SystemComponentStatus.Unavailable => "当前安装包不可用",
            SystemComponentStatus.Failed => "异常",
            _ => "未知",
        };
    }
}
