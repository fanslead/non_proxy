using NonProxy.Control.V1;
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

    public async Task<DiagnosticExport> ExportAsync(
        CancellationToken cancellationToken)
    {
        var response = await _client.ExportDiagnosticsAsync(cancellationToken);
        if (response.Error is { } error)
        {
            throw new ControlServiceException(
                error.Code,
                ExportErrorMessage(error.Code));
        }
        if (string.IsNullOrWhiteSpace(response.DiagnosticId)
            || string.IsNullOrWhiteSpace(response.LocalPath)
            || !Path.IsPathFullyQualified(response.LocalPath)
            || response.SizeBytes == 0
            || response.SizeBytes > long.MaxValue
            || response.Sha256.Length != 32
            || response.AppliedRedactionLevel != DiagnosticRedactionLevel.Strict
            || response.EffectiveTimeRange?.Start is null
            || response.EffectiveTimeRange.End is null
            || response.IncludedSections.Count == 0
            || response.ConnectionSampleCount != 0
            || response.ErrorCount > int.MaxValue)
        {
            throw new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "控制服务没有返回完整且严格脱敏的诊断包结果。");
        }
        var start = response.EffectiveTimeRange.Start.ToDateTimeOffset();
        var end = response.EffectiveTimeRange.End.ToDateTimeOffset();
        if (start >= end)
        {
            throw new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "控制服务返回的诊断时间范围无效。");
        }

        return new DiagnosticExport(
            response.DiagnosticId,
            response.LocalPath,
            checked((long)response.SizeBytes),
            Convert.ToHexString(response.Sha256.Span).ToLowerInvariant(),
            "严格脱敏",
            start,
            end,
            response.IncludedSections.Select(SectionLabel).ToArray(),
            checked((int)response.ConnectionSampleCount),
            checked((int)response.ErrorCount));
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

    private static string ExportErrorMessage(string code)
    {
        return code switch
        {
            "NP_DIAGNOSTICS_PATH_UNSAFE" =>
                "诊断目录状态不安全，后台服务没有写入任何文件。",
            "NP_DIAGNOSTICS_EXPORT_FAILED" =>
                "诊断包生成失败，请检查磁盘空间和目录权限后重试。",
            "NP_FEATURE_NOT_AVAILABLE" =>
                "当前后台服务版本尚不支持导出诊断包。",
            _ => "诊断包没有生成，请稍后重试。",
        };
    }

    private static string SectionLabel(string section)
    {
        return section switch
        {
            "runtime" => "组件版本、系统与能力",
            "configuration_summary" => "规则与出口聚合统计",
            "component_states" => "后台组件状态",
            "network_and_route_summary" => "网络路径与默认路由摘要",
            "recent_errors" => "最近稳定错误码",
            "connection_samples" => "脱敏连接样本",
            _ => "其他兼容诊断信息",
        };
    }

}
