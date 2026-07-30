using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Policies;

public sealed partial class PoliciesViewModel : LoadableViewModel
{
    private readonly IPolicyService _policyService;

    [ObservableProperty]
    private string? _operationMessage;

    public PoliciesViewModel(IPolicyService policyService)
        : base("全部规则")
    {
        _policyService = policyService;
        DeleteCommand = new AsyncRelayCommand<PolicyListItem>(
            DeleteAsync,
            CanDelete);
    }

    public ObservableCollection<PolicyListItem> Items { get; } = [];

    public IAsyncRelayCommand<PolicyListItem> DeleteCommand { get; }

    public string ActiveSnapshotLabel { get; private set; } = "尚无已激活快照";

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var catalog = await _policyService.GetCatalogAsync(cancellationToken);

        Items.Clear();
        foreach (var item in catalog.Items
                     .OrderByDescending(item => item.UpdatedAt ?? DateTimeOffset.MinValue))
        {
            Items.Add(item);
        }

        ActiveSnapshotLabel = (catalog.ActiveSnapshotVersion, catalog.PendingSnapshotVersion) switch
        {
            ({ } active, { } pending) => $"当前生效：快照 v{active}；等待确认：v{pending}",
            ({ } active, null) => $"当前生效：快照 v{active}",
            (null, { } pending) => $"尚无已激活快照；等待确认：v{pending}",
            _ => "尚无已激活快照",
        };
        OnPropertyChanged(nameof(ActiveSnapshotLabel));
    }

    private bool CanDelete(PolicyListItem? item)
    {
        return !IsBusy
            && item is not null
            && item.State != PolicyApplyState.PendingRemoval;
    }

    private Task DeleteAsync(
        PolicyListItem? item,
        CancellationToken cancellationToken)
    {
        if (item is null)
        {
            return Task.CompletedTask;
        }

        return RunOperationAsync(
            async token =>
            {
                var result = await _policyService.DeleteAsync(item.Id, token);
                OperationMessage = result.Message;
                await LoadCoreAsync(token);
            },
            cancellationToken);
    }
}
