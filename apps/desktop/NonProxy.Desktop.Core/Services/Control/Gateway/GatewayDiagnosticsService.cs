using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayDiagnosticsService : IDiagnosticsService
{
    private readonly IControlRpcClient _client;
    private readonly ISystemComponentInstaller _installer;

    public GatewayDiagnosticsService(
        IControlRpcClient client,
        ISystemComponentInstaller installer)
    {
        _client = client;
        _installer = installer;
    }

    public async Task<IReadOnlyList<DiagnosticCheck>> RunChecksAsync(
        CancellationToken cancellationToken)
    {
        var checks = new List<DiagnosticCheck>();
        try
        {
            var status = await _client.GetSystemStatusAsync(cancellationToken);
            checks.Add(new DiagnosticCheck(
                "control-service",
                "控制服务",
                "已连接",
                $"事件游标 {status.LatestEventSequence}，"
                + SnapshotDetail(
                    status.ActiveSnapshotVersion,
                    status.PendingSnapshotVersion)));
        }
        catch (ControlServiceException exception)
        {
            checks.Add(new DiagnosticCheck(
                "control-service",
                "控制服务",
                "未连接",
                exception.UserMessage));
        }

        var component = await _installer.GetStateAsync(cancellationToken);
        checks.Add(new DiagnosticCheck(
            "system-component",
            "系统网络组件",
            ToStatusLabel(component.Status),
            component.Message));
        return checks;
    }

    private static string SnapshotDetail(ulong active, ulong pending)
    {
        return (active, pending) switch
        {
            ( > 0, > 0) => $"生效快照 v{active}，待确认 v{pending}",
            ( > 0, 0) => $"生效快照 v{active}",
            (0, > 0) => $"待确认快照 v{pending}",
            _ => "尚无快照",
        };
    }

    private static string ToStatusLabel(SystemComponentStatus status)
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
