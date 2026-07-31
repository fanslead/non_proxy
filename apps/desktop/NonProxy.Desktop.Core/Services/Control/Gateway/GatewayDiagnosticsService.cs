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
            checks.Add(DecisionTelemetryCheck(
                status.DroppedDecisionEvents));
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
        SystemComponentDiagnostics.AddTo(checks, component);
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

    private static DiagnosticCheck DecisionTelemetryCheck(ulong dropped)
    {
        return dropped == 0
            ? new DiagnosticCheck(
                "decision-evidence",
                "连接路径证据",
                "正常",
                "本次后台服务运行期间未检测到决策事件丢失。")
            : new DiagnosticCheck(
                "decision-evidence",
                "连接路径证据",
                "需关注",
                $"本次后台服务运行期间有 {dropped} 条决策事件因报告队列不可用而丢失；"
                + "网络转发不受影响，但活动记录并不完整。");
    }

}
