using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed partial class OutboundGroupsViewModel
{
    private bool CanSave()
    {
        return IsEditorVisible
            && !IsBusy
            && !string.IsNullOrWhiteSpace(EditingName)
            && PriorityMembers.Count is >= 2 and <= 32;
    }

    private async Task SaveAsync(CancellationToken cancellationToken)
    {
        if (!CanSave() || _editingId is null)
        {
            ValidationMessage = "请填写名称，并按优先级选择至少 2 条线路。";
            return;
        }
        await RunOperationAsync(async token =>
        {
            var result = await _service.SaveAsync(
                new OutboundGroupDraft(
                    _editingId,
                    EditingName,
                    PriorityMembers.Select(member => member.OutboundId).ToArray(),
                    _editingRevision),
                token);
            OperationMessage = result.Message;
            if (!result.Accepted || result.Group is null)
            {
                return;
            }
            var presentedGroup = WithMemberNames(result.Group);
            ReplaceGroup(presentedGroup);
            CancelEdit();
            OperationMessage = result.Message;
            if (presentedGroup.IsDefault)
            {
                ApplyDefaultGroup(presentedGroup, result.RoutingRevision);
            }
        }, cancellationToken);
    }

    private bool CanSetDefault(OutboundGroupListItem? group)
    {
        return group is { IsDefault: false }
            && _routingRevision > 0
            && group.OutboundIds.All(id => AvailableOutbounds.Any(outbound =>
                string.Equals(outbound.Id, id, StringComparison.Ordinal)))
            && !IsBusy;
    }

    private async Task SetDefaultAsync(
        OutboundGroupListItem? group,
        CancellationToken cancellationToken)
    {
        if (!CanSetDefault(group))
        {
            return;
        }
        await RunOperationAsync(async token =>
        {
            var result = await _service.SetDefaultAsync(
                group!.Id,
                _routingRevision,
                token);
            OperationMessage = result.Message;
            if (result.Accepted)
            {
                ApplyDefaultGroup(group, _routingRevision + 1);
            }
        }, cancellationToken);
    }

    private async Task ConfirmDeleteAsync(CancellationToken cancellationToken)
    {
        var group = PendingDeleteGroup;
        PendingDeleteGroup = null;
        if (group is null)
        {
            return;
        }
        await RunOperationAsync(async token =>
        {
            var result = await _service.DeleteAsync(group.Id, group.Revision, token);
            OperationMessage = result.Message;
            if (result.Accepted)
            {
                var current = Groups.SingleOrDefault(item => string.Equals(
                    item.Id,
                    group.Id,
                    StringComparison.Ordinal));
                if (current is not null)
                {
                    Groups.Remove(current);
                }
                OnPropertyChanged(nameof(HasGroups));
            }
        }, cancellationToken);
    }

    private void ReplaceGroup(OutboundGroupListItem group)
    {
        var existing = Groups.SingleOrDefault(item => string.Equals(
            item.Id,
            group.Id,
            StringComparison.Ordinal));
        if (existing is not null)
        {
            Groups.Remove(existing);
        }
        Groups.Add(group);
        var sorted = Groups.OrderBy(item => item.Name).ToArray();
        Groups.Clear();
        foreach (var item in sorted)
        {
            Groups.Add(item);
        }
        OnPropertyChanged(nameof(HasGroups));
    }

    private void ApplyDefaultGroup(OutboundGroupListItem selected, ulong revision)
    {
        _routingRevision = revision;
        for (var index = 0; index < Groups.Count; index++)
        {
            Groups[index] = Groups[index] with
            {
                IsDefault = string.Equals(
                    Groups[index].Id,
                    selected.Id,
                    StringComparison.Ordinal),
            };
        }
        NotifyCommands();
        DefaultRouteChanged?.Invoke(
            this,
            new OutboundGroupDefaultRouteChange(
                selected.Id,
                selected.Name,
                revision));
    }

    private async Task RunOperationAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        try
        {
            IsBusy = true;
            ValidationMessage = null;
            await operation(cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (ControlServiceException exception)
        {
            ValidationMessage = exception.UserMessage;
        }
        catch (Exception)
        {
            ValidationMessage = "线路组操作未完成，请刷新后重试。";
        }
        finally
        {
            IsBusy = false;
        }
    }

    private void NotifyCommands()
    {
        StartCreateCommand.NotifyCanExecuteChanged();
        EditCommand.NotifyCanExecuteChanged();
        AddMemberCommand.NotifyCanExecuteChanged();
        RemoveMemberCommand.NotifyCanExecuteChanged();
        MoveMemberUpCommand.NotifyCanExecuteChanged();
        MoveMemberDownCommand.NotifyCanExecuteChanged();
        SaveCommand.NotifyCanExecuteChanged();
        SetDefaultCommand.NotifyCanExecuteChanged();
        RequestDeleteCommand.NotifyCanExecuteChanged();
        ConfirmDeleteCommand.NotifyCanExecuteChanged();
    }
}
