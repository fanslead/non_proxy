using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewaySystemStatusService : ISystemStatusService
{
    private readonly IControlRpcClient _client;
    private readonly IPolicyService _policies;
    private readonly ISystemComponentInstaller _installer;

    public GatewaySystemStatusService(
        IControlRpcClient client,
        IPolicyService policies,
        ISystemComponentInstaller installer)
    {
        _client = client;
        _policies = policies;
        _installer = installer;
    }

    public async Task<SystemOverview> GetOverviewAsync(
        CancellationToken cancellationToken)
    {
        var component = await _installer.GetStateAsync(cancellationToken);
        try
        {
            var statusTask = _client.GetSystemStatusAsync(cancellationToken);
            var policiesTask = _policies.GetCatalogAsync(cancellationToken);
            var decisionsTask = _client.ListConnectionDecisionsAsync(
                1,
                string.Empty,
                cancellationToken);
            await Task.WhenAll(statusTask, policiesTask, decisionsTask);
            var status = await statusTask;
            var catalog = await policiesTask;
            var decisions = await decisionsTask;
            var visible = catalog.Items.Where(item =>
                item.State != PolicyApplyState.PendingRemoval);
            var applicationCount = visible.Count(item =>
                item.Action == PolicyAction.Direct
                && (item.Scope is PolicyScope.Application
                    or PolicyScope.ApplicationAndDestination));
            var websiteCount = visible.Count(item =>
                item.Action == PolicyAction.Direct
                && item.Scope == PolicyScope.Website);
            var networkCount = visible.Count(item =>
                item.Action == PolicyAction.Direct
                && item.Scope == PolicyScope.Network);
            var activeDirectCount = catalog.Items.Count(item =>
                item.Action == PolicyAction.Direct
                && item.State == PolicyApplyState.Active);
            return new SystemOverview(
                ConnectionState.Connected,
                component,
                Headline(status.DataPlaneEnabled, status.PendingSnapshotVersion),
                Detail(status.DataPlaneEnabled, status.ActiveSnapshotVersion),
                OptionalVersion(status.ActiveSnapshotVersion),
                applicationCount,
                websiteCount,
                networkCount,
                DecisionCount(decisions.TotalCount),
                DateTimeOffset.UtcNow,
                OptionalVersion(status.PendingSnapshotVersion),
                status.DataPlaneEnabled,
                activeDirectCount);
        }
        catch (ControlServiceException exception)
        {
            return new SystemOverview(
                ConnectionState.Disconnected,
                component,
                "等待控制服务",
                exception.UserMessage,
                null,
                0,
                0,
                0,
                0,
                DateTimeOffset.UtcNow);
        }
    }

    private static string Headline(bool dataPlaneEnabled, ulong pendingVersion)
    {
        if (dataPlaneEnabled)
        {
            return "隔离路由正在运行";
        }

        return pendingVersion > 0
            ? "策略已送达，等待系统组件确认"
            : "控制服务已连接";
    }

    private static string Detail(bool dataPlaneEnabled, ulong activeVersion)
    {
        if (dataPlaneEnabled && activeVersion > 0)
        {
            return "控制面与数据面状态同步正常。";
        }

        return "当前没有已激活的数据面快照，系统流量尚未由 NonProxy 接管。";
    }

    private static ulong? OptionalVersion(ulong value)
    {
        return value == 0 ? null : value;
    }

    private static int DecisionCount(ulong value)
    {
        return value > int.MaxValue ? int.MaxValue : checked((int)value);
    }
}
