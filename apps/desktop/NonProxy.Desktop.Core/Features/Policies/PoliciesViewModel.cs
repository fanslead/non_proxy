using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Policies;

public sealed partial class PoliciesViewModel : LoadableViewModel
{
    private readonly IPolicyService _policyService;
    private ulong? _restoreSnapshotVersion;
    private ulong? _pendingSnapshotVersion;

    [ObservableProperty]
    private string? _operationMessage;

    [ObservableProperty]
    private bool _isRestoreConfirmationVisible;

    public PoliciesViewModel(IPolicyService policyService)
        : base("全部规则")
    {
        _policyService = policyService;
        DeleteCommand = new AsyncRelayCommand<PolicyListItem>(
            DeleteAsync,
            CanDelete);
        RequestRestorePreviousCommand = new RelayCommand(RequestRestorePrevious);
        CancelRestorePreviousCommand = new RelayCommand(
            () => IsRestoreConfirmationVisible = false);
        ConfirmRestorePreviousCommand = new AsyncRelayCommand(
            ConfirmRestorePreviousAsync,
            AsyncRelayCommandOptions.None);
    }

    public ObservableCollection<PolicyListItem> Items { get; } = [];

    public IAsyncRelayCommand<PolicyListItem> DeleteCommand { get; }

    public IRelayCommand RequestRestorePreviousCommand { get; }

    public IRelayCommand CancelRestorePreviousCommand { get; }

    public IAsyncRelayCommand ConfirmRestorePreviousCommand { get; }

    public string ActiveSnapshotLabel { get; private set; } = "尚无已激活快照";

    public string RestorePreviousDetail { get; private set; } = "正在读取恢复点。";

    public bool CanRestorePrevious =>
        _restoreSnapshotVersion is not null && _pendingSnapshotVersion is null;

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
        var previousRestoreSnapshotVersion = _restoreSnapshotVersion;
        _restoreSnapshotVersion = catalog.PreviousEffectiveSnapshotVersion;
        _pendingSnapshotVersion = catalog.PendingSnapshotVersion;
        RestorePreviousDetail = (catalog.PreviousEffectiveSnapshotVersion, catalog.PendingSnapshotVersion) switch
        {
            (_, not null) => "当前有配置等待系统组件确认，完成后才能选择恢复点。",
            ({ } previous, null) => $"可恢复到上一次真正生效的快照 v{previous}；恢复也需要系统组件重新确认。",
            _ => "尚无更早的已生效配置可以恢复。",
        };
        if (!CanRestorePrevious
            || previousRestoreSnapshotVersion != _restoreSnapshotVersion)
        {
            IsRestoreConfirmationVisible = false;
        }
        OnPropertyChanged(nameof(ActiveSnapshotLabel));
        OnPropertyChanged(nameof(RestorePreviousDetail));
        OnPropertyChanged(nameof(CanRestorePrevious));
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

    private void RequestRestorePrevious()
    {
        if (CanRestorePrevious)
        {
            IsRestoreConfirmationVisible = true;
        }
    }

    private Task ConfirmRestorePreviousAsync(CancellationToken cancellationToken)
    {
        if (!IsRestoreConfirmationVisible
            || _restoreSnapshotVersion is not { } snapshotVersion)
        {
            return Task.CompletedTask;
        }
        IsRestoreConfirmationVisible = false;

        return RunOperationAsync(
            async token =>
            {
                var result = await _policyService.RollBackAsync(
                    snapshotVersion,
                    token);
                OperationMessage = result.Message;
                await LoadCoreAsync(token);
                if (!result.Accepted)
                {
                    ErrorMessage = result.Message;
                }
            },
            cancellationToken);
    }
}
