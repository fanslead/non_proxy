namespace NonProxy.Desktop.Core.Features.Subscriptions;

public sealed partial class SubscriptionsViewModel
{
    private void RequestDelete(SubscriptionViewItem? item)
    {
        if (item is null || IsBusy)
        {
            return;
        }
        ClearPendingDelete();
        item.IsDeletePending = true;
        CloseEditor();
        NotifyCommandState();
    }

    private void CancelDelete(SubscriptionViewItem? item)
    {
        if (item is not null)
        {
            item.IsDeletePending = false;
        }
        NotifyCommandState();
    }

    private bool CanConfirmDelete(SubscriptionViewItem? item)
    {
        return !IsBusy && item is { IsDeletePending: true };
    }

    private async Task ConfirmDeleteAsync(
        SubscriptionViewItem? item,
        CancellationToken cancellationToken)
    {
        if (item is not { IsDeletePending: true } || IsBusy)
        {
            return;
        }
        await RunSubscriptionOperationAsync(
            async token =>
            {
                var result = await _subscriptionService.DeleteAsync(
                    item.Id,
                    item.Revision,
                    token);
                if (!result.Accepted)
                {
                    ErrorMessage = result.Message;
                    return;
                }
                OperationMessage = WithWarnings(result.Message, result.Warnings);
                await LoadCoreAsync(token);
            },
            cancellationToken);
    }

    private void ClearPendingDelete()
    {
        foreach (var item in Items)
        {
            item.IsDeletePending = false;
        }
    }
}
