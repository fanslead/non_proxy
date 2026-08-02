using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed partial class OutboundsViewModel
{
    protected override async Task LoadCoreAsync(
        CancellationToken cancellationToken)
    {
        for (var attempt = 0; attempt < 2; attempt++)
        {
            var outboundTask = _outboundService.ListAsync(cancellationToken);
            var groupTask = _outboundGroupService.ListAsync(cancellationToken);
            await Task.WhenAll(outboundTask, groupTask);
            var catalog = await outboundTask;
            var groups = await groupTask;
            if (CatalogsAgree(catalog, groups))
            {
                ApplyCatalog(catalog, groups);
                return;
            }
        }

        throw new ControlServiceException(
            "NP_CONTROL_CATALOG_INCONSISTENT",
            "默认路由刚刚发生变化，请稍后重新检查网络出口。");
    }

    private static bool CatalogsAgree(
        OutboundCatalog catalog,
        OutboundGroupCatalog groups)
    {
        if (groups.RoutingRevision == 0)
        {
            return groups.Groups.Count == 0
                && groups.DefaultGroupId is null
                && catalog.DefaultOutboundGroupId is null;
        }
        return catalog.RoutingRevision == groups.RoutingRevision
            && string.Equals(
                catalog.DefaultOutboundGroupId,
                groups.DefaultGroupId,
                StringComparison.Ordinal);
    }

    private void ApplyCatalog(
        OutboundCatalog catalog,
        OutboundGroupCatalog groups)
    {
        _routingRevision = catalog.RoutingRevision;
        _usesDirectByDefault = catalog.UsesDirectByDefault;
        ExitVerificationAvailable = catalog.ExitVerificationAvailable;
        DirectExitReceipt = catalog.DirectExitReceipt;
        DefaultRouteSummary = BuildDefaultRouteSummary(catalog, groups);
        Items.Clear();
        foreach (var item in catalog.Items.OrderBy(item => item.Name))
        {
            Items.Add(item);
        }
        OutboundGroups.ApplyCatalog(groups, catalog.Items);
        NotifyRouteCommands();
    }

    private static string BuildDefaultRouteSummary(
        OutboundCatalog catalog,
        OutboundGroupCatalog groups)
    {
        if (catalog.RoutingRevision == 0)
        {
            return "配置：暂时无法读取";
        }
        if (catalog.DefaultOutboundId is { } outboundId)
        {
            return $"配置：未命中规则时使用代理 {outboundId}";
        }
        if (catalog.DefaultOutboundGroupId is { } groupId)
        {
            var name = groups.Groups.SingleOrDefault(group =>
                string.Equals(group.Id, groupId, StringComparison.Ordinal))?.Name;
            return $"配置：未命中规则时使用自动切换线路组 {name ?? groupId}";
        }
        return "配置：未命中规则时默认直连";
    }

    private void ApplyGroupDefaultRoute(
        object? sender,
        OutboundGroupDefaultRouteChange args)
    {
        _routingRevision = args.RoutingRevision;
        _usesDirectByDefault = false;
        for (var index = 0; index < Items.Count; index++)
        {
            Items[index] = Items[index] with { IsDefault = false };
        }
        DefaultRouteSummary =
            $"配置：未命中规则时使用自动切换线路组 {args.GroupName}";
        NotifyRouteCommands();
    }

    private void NotifyRouteCommands()
    {
        SetDefaultCommand.NotifyCanExecuteChanged();
        SetDirectCommand.NotifyCanExecuteChanged();
        VerifyExitCommand.NotifyCanExecuteChanged();
        VerifyDirectExitCommand.NotifyCanExecuteChanged();
    }
}
