using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Subscriptions;

public sealed partial class SubscriptionsViewModel : LoadableViewModel
{
    private readonly ISubscriptionService _subscriptionService;
    private string? _editingId;
    private ulong? _editingRevision;

    [ObservableProperty]
    private bool _isEditorOpen;

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    private string _displayName = string.Empty;

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    private string _endpointUrl = string.Empty;

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    private SubscriptionIntervalOption _selectedInterval = IntervalOptions[2];

    [ObservableProperty]
    private bool _subscriptionEnabled = true;

    [ObservableProperty]
    private string? _validationMessage;

    [ObservableProperty]
    private string? _operationMessage;

    [ObservableProperty]
    private int _activeCount;

    [ObservableProperty]
    private int _attentionCount;

    [ObservableProperty]
    private int _disabledCount;

    [ObservableProperty]
    private bool _hasItems;

    public SubscriptionsViewModel(ISubscriptionService subscriptionService)
        : base("订阅管理")
    {
        _subscriptionService = subscriptionService;
        OpenCreateCommand = new RelayCommand(OpenCreate, () => !IsBusy);
        EditCommand = new RelayCommand<SubscriptionViewItem>(Edit, CanMutateItem);
        CancelEditorCommand = new RelayCommand(CloseEditor);
        SaveCommand = new AsyncRelayCommand(SaveAsync, CanSave);
        RefreshSourceCommand = new AsyncRelayCommand<SubscriptionViewItem>(
            RefreshSourceAsync,
            CanMutateItem);
        ToggleEnabledCommand = new AsyncRelayCommand<SubscriptionViewItem>(
            ToggleEnabledAsync,
            CanMutateItem);
        RequestDeleteCommand = new RelayCommand<SubscriptionViewItem>(
            RequestDelete,
            CanMutateItem);
        CancelDeleteCommand = new RelayCommand<SubscriptionViewItem>(CancelDelete);
        ConfirmDeleteCommand = new AsyncRelayCommand<SubscriptionViewItem>(
            ConfirmDeleteAsync,
            CanConfirmDelete);
        PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(IsBusy))
            {
                NotifyCommandState();
            }
        };
    }

    public static IReadOnlyList<SubscriptionIntervalOption> IntervalOptions { get; } =
    [
        new("15 分钟", TimeSpan.FromMinutes(15), "适合临时或高频变化订阅"),
        new("1 小时", TimeSpan.FromHours(1), "快速跟进节点变化"),
        new("6 小时", TimeSpan.FromHours(6), "推荐，兼顾新鲜度与服务负载"),
        new("12 小时", TimeSpan.FromHours(12), "每日检查两次"),
        new("1 天", TimeSpan.FromDays(1), "适合长期稳定订阅"),
        new("3 天", TimeSpan.FromDays(3), "低频变更"),
        new("7 天", TimeSpan.FromDays(7), "允许的最长间隔"),
    ];

    public ObservableCollection<SubscriptionViewItem> Items { get; } = [];

    public ObservableCollection<SubscriptionIntervalOption> AvailableIntervals { get; } =
        new(IntervalOptions);

    public IRelayCommand OpenCreateCommand { get; }

    public IRelayCommand<SubscriptionViewItem> EditCommand { get; }

    public IRelayCommand CancelEditorCommand { get; }

    public IAsyncRelayCommand SaveCommand { get; }

    public IAsyncRelayCommand<SubscriptionViewItem> RefreshSourceCommand { get; }

    public IAsyncRelayCommand<SubscriptionViewItem> ToggleEnabledCommand { get; }

    public IRelayCommand<SubscriptionViewItem> RequestDeleteCommand { get; }

    public IRelayCommand<SubscriptionViewItem> CancelDeleteCommand { get; }

    public IAsyncRelayCommand<SubscriptionViewItem> ConfirmDeleteCommand { get; }

    public bool IsEditing => _editingRevision is not null;

    public bool IsEmpty => !HasItems;

    public string EditorTitle => IsEditing ? "调整订阅" : "添加远程订阅";

    public string SaveActionLabel => IsEditing ? "保存设置" : "安全添加并检查";

    public string EndpointHint => IsEditing
        ? "留空会继续使用系统凭据库中的原地址；只有更换地址时才需要重新粘贴。"
        : "只接受 HTTPS；地址可能包含 Token，保存后不会在列表、日志或诊断中回显。";

    public string SyncSummary => Items.Count == 0
        ? "尚未添加远程订阅"
        : $"{ActiveCount} 个自动同步 · {AttentionCount} 个需处理 · {DisabledCount} 个已停用";

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var catalog = await _subscriptionService.ListAsync(cancellationToken);
        Items.Clear();
        foreach (var item in catalog.Items.OrderBy(item => item.DisplayName))
        {
            Items.Add(new SubscriptionViewItem(item));
        }
        Recount();
        NotifyCommandState();
    }

    private void OpenCreate()
    {
        ClearPendingDelete();
        _editingId = $"subscription-{Guid.NewGuid():N}";
        _editingRevision = null;
        ResetAvailableIntervals();
        DisplayName = string.Empty;
        EndpointUrl = string.Empty;
        SelectedInterval = IntervalOptions[2];
        SubscriptionEnabled = true;
        ValidationMessage = null;
        IsEditorOpen = true;
        NotifyEditorState();
    }

    private void Edit(SubscriptionViewItem? item)
    {
        if (item is null || IsBusy)
        {
            return;
        }
        ClearPendingDelete();
        _editingId = item.Id;
        _editingRevision = item.Revision;
        DisplayName = item.DisplayName;
        EndpointUrl = string.Empty;
        ResetAvailableIntervals();
        SelectedInterval = AvailableIntervals.FirstOrDefault(option =>
            option.Value == item.Source.RefreshInterval)
            ?? new SubscriptionIntervalOption(
                item.IntervalLabel,
                item.Source.RefreshInterval,
                "当前保存的自定义间隔");
        if (!AvailableIntervals.Contains(SelectedInterval))
        {
            AvailableIntervals.Add(SelectedInterval);
        }
        SubscriptionEnabled = item.Enabled;
        ValidationMessage = null;
        IsEditorOpen = true;
        NotifyEditorState();
    }

    private void CloseEditor()
    {
        EndpointUrl = string.Empty;
        DisplayName = string.Empty;
        ValidationMessage = null;
        _editingId = null;
        _editingRevision = null;
        IsEditorOpen = false;
        NotifyEditorState();
    }

    private bool CanSave()
    {
        return !IsBusy
            && IsEditorOpen
            && !string.IsNullOrWhiteSpace(DisplayName)
            && (IsEditing || !string.IsNullOrWhiteSpace(EndpointUrl));
    }

    private async Task SaveAsync(CancellationToken cancellationToken)
    {
        if (!CanSave() || _editingId is null)
        {
            return;
        }
        OperationMessage = null;
        if (!ValidateEditor())
        {
            return;
        }

        await RunSubscriptionOperationAsync(
            async token =>
            {
                ValidationMessage = null;
                var result = await _subscriptionService.SaveAsync(
                    new SubscriptionDraft(
                        _editingId,
                        DisplayName,
                        string.IsNullOrWhiteSpace(EndpointUrl) ? null : EndpointUrl,
                        SubscriptionEnabled,
                        SelectedInterval.Value,
                        _editingRevision),
                    token);
                EndpointUrl = string.Empty;
                if (!result.Accepted)
                {
                    ErrorMessage = result.Message;
                    return;
                }

                OperationMessage = WithWarnings(result.Message, result.Warnings);
                CloseEditor();
                await LoadCoreAsync(token);
            },
            cancellationToken);
    }

    private async Task RefreshSourceAsync(
        SubscriptionViewItem? item,
        CancellationToken cancellationToken)
    {
        if (item is null || IsBusy)
        {
            return;
        }
        await RunSubscriptionOperationAsync(
            async token =>
            {
                var result = await _subscriptionService.RefreshAsync(
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

    private async Task ToggleEnabledAsync(
        SubscriptionViewItem? item,
        CancellationToken cancellationToken)
    {
        if (item is null || IsBusy)
        {
            return;
        }
        await RunSubscriptionOperationAsync(
            async token =>
            {
                var result = await _subscriptionService.SaveAsync(
                    new SubscriptionDraft(
                        item.Id,
                        item.DisplayName,
                        null,
                        !item.Enabled,
                        item.Source.RefreshInterval,
                        item.Revision),
                    token);
                if (!result.Accepted)
                {
                    ErrorMessage = result.Message;
                    return;
                }
                OperationMessage = item.Enabled
                    ? "订阅已停用；现有节点保留，但不会自动刷新。"
                    : "订阅已重新启用，后台将立即检查一次。";
                await LoadCoreAsync(token);
            },
            cancellationToken);
    }

    private bool CanMutateItem(SubscriptionViewItem? item)
    {
        return !IsBusy && item is not null;
    }

    private async Task RunSubscriptionOperationAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        OperationMessage = null;
        await RunOperationAsync(
            async token =>
            {
                NotifyCommandState();
                await operation(token);
            },
            cancellationToken);
        NotifyCommandState();
    }

    private void Recount()
    {
        ActiveCount = Items.Count(item => item.Enabled);
        AttentionCount = Items.Count(item => item.NeedsAttention);
        DisabledCount = Items.Count(item => item.IsDisabled);
        HasItems = Items.Count > 0;
        OnPropertyChanged(nameof(IsEmpty));
        OnPropertyChanged(nameof(SyncSummary));
    }

    private void ResetAvailableIntervals()
    {
        AvailableIntervals.Clear();
        foreach (var option in IntervalOptions)
        {
            AvailableIntervals.Add(option);
        }
    }

    private void NotifyEditorState()
    {
        OnPropertyChanged(nameof(IsEditing));
        OnPropertyChanged(nameof(EditorTitle));
        OnPropertyChanged(nameof(SaveActionLabel));
        OnPropertyChanged(nameof(EndpointHint));
        SaveCommand.NotifyCanExecuteChanged();
    }

    private void NotifyCommandState()
    {
        OpenCreateCommand.NotifyCanExecuteChanged();
        EditCommand.NotifyCanExecuteChanged();
        SaveCommand.NotifyCanExecuteChanged();
        RefreshSourceCommand.NotifyCanExecuteChanged();
        ToggleEnabledCommand.NotifyCanExecuteChanged();
        RequestDeleteCommand.NotifyCanExecuteChanged();
        ConfirmDeleteCommand.NotifyCanExecuteChanged();
    }

    private static string WithWarnings(string message, IReadOnlyList<string> warnings)
    {
        return warnings.Count == 0
            ? message
            : $"{message} {string.Join("；", warnings)}";
    }
}
