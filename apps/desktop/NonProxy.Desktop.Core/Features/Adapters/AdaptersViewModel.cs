using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;

namespace NonProxy.Desktop.Core.Features.Adapters;

public sealed partial class AdaptersViewModel : LoadableViewModel
{
    private readonly IAdapterManagementService _adapterService;
    private readonly IAdapterFilePicker _filePicker;
    private string? _suggestedManagedRulesPath;

    [ObservableProperty]
    private AdapterClientOption _selectedClient = ClientOptions[0];

    [ObservableProperty]
    private string _adapterId = ClientOptions[0].DefaultId;

    [ObservableProperty]
    private string _executablePath = string.Empty;

    [ObservableProperty]
    private string _mainConfigurationPath = string.Empty;

    [ObservableProperty]
    private string _managedRulesPath = string.Empty;

    [ObservableProperty]
    private string _directTarget = string.Empty;

    [ObservableProperty]
    private string? _validationMessage;

    [ObservableProperty]
    private string? _operationMessage;

    [ObservableProperty]
    private bool _lastClientValidated;

    [ObservableProperty]
    private bool _lastConfigurationVerified;

    [ObservableProperty]
    private bool _lastPathVerified;

    [ObservableProperty]
    private string _lastSnapshotLabel = "活动快照：尚未同步";

    [ObservableProperty]
    private string _lastRuleCountLabel = "规则：—";

    [ObservableProperty]
    private string _lastEvidenceMessage = "完成客户端同步后，这里会分别显示候选校验、配置载入和真实路径证据。";

    public AdaptersViewModel(
        IAdapterManagementService adapterService,
        IAdapterFilePicker filePicker)
        : base("客户端协同")
    {
        _adapterService = adapterService;
        _filePicker = filePicker;
        ChooseExecutableCommand = new AsyncRelayCommand(
            ChooseExecutableAsync,
            () => !IsBusy);
        ChooseConfigurationCommand = new AsyncRelayCommand(
            ChooseConfigurationAsync,
            () => !IsBusy);
        RegisterCommand = new AsyncRelayCommand(RegisterAsync, CanRegister);
        SyncCommand = new AsyncRelayCommand<AdapterInstallationItem>(
            SyncAsync,
            CanManage);
        RemoveCommand = new AsyncRelayCommand<AdapterInstallationItem>(
            RemoveAsync,
            CanManage);
    }

    public ObservableCollection<AdapterInstallationItem> Items { get; } = [];

    public ObservableCollection<AdapterProjectionBlocker> Blockers { get; } = [];

    public bool HasItems => Items.Count > 0;

    public bool HasNoItems => !HasItems;

    public bool HasBlockers => Blockers.Count > 0;

    public string ClientValidationLabel => LastClientValidated ? "已校验" : "未确认";

    public string ConfigurationVerificationLabel =>
        LastConfigurationVerified ? "已载入" : "未确认";

    public string PathVerificationLabel =>
        LastPathVerified ? "已证明直连" : "尚未证明绕过 VPN";

    public string ClientExecutableHint => SelectedClient.ExecutableHint;

    public string ClientConfigurationHint => SelectedClient.ConfigurationHint;

    public string ClientDirectTargetHint => SelectedClient.DirectTargetHint;

    public IAsyncRelayCommand RegisterCommand { get; }

    public IAsyncRelayCommand ChooseExecutableCommand { get; }

    public IAsyncRelayCommand ChooseConfigurationCommand { get; }

    public IAsyncRelayCommand<AdapterInstallationItem> SyncCommand { get; }

    public IAsyncRelayCommand<AdapterInstallationItem> RemoveCommand { get; }

    partial void OnSelectedClientChanged(AdapterClientOption value)
    {
        AdapterId = value.DefaultId;
        RefreshManagedRulesSuggestion();
        OnPropertyChanged(nameof(ClientExecutableHint));
        OnPropertyChanged(nameof(ClientConfigurationHint));
        OnPropertyChanged(nameof(ClientDirectTargetHint));
        InputChanged();
    }

    partial void OnAdapterIdChanged(string value)
    {
        InputChanged();
    }

    partial void OnExecutablePathChanged(string value)
    {
        InputChanged();
    }

    partial void OnMainConfigurationPathChanged(string value)
    {
        RefreshManagedRulesSuggestion();
        InputChanged();
    }

    partial void OnManagedRulesPathChanged(string value)
    {
        InputChanged();
    }

    partial void OnDirectTargetChanged(string value)
    {
        InputChanged();
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var catalog = await _adapterService.ListAsync(cancellationToken);
        Items.Clear();
        foreach (var item in catalog.Items.OrderBy(item => item.ClientName))
        {
            Items.Add(item);
        }
        NotifyCollectionState();
    }

    protected override void OnBusyStateChanged()
    {
        NotifyOperationCommands();
    }

    private bool CanRegister()
    {
        return !IsBusy
            && !string.IsNullOrWhiteSpace(AdapterId)
            && !string.IsNullOrWhiteSpace(ExecutablePath)
            && !string.IsNullOrWhiteSpace(MainConfigurationPath)
            && !string.IsNullOrWhiteSpace(ManagedRulesPath);
    }

    private async Task RegisterAsync(CancellationToken cancellationToken)
    {
        await RunAdapterOperationAsync(
            async token =>
            {
                ValidationMessage = AdapterRegistrationValidator.Validate(
                    AdapterId,
                    ExecutablePath,
                    MainConfigurationPath,
                    ManagedRulesPath,
                    DirectTarget);
                if (ValidationMessage is not null)
                {
                    return;
                }

                var result = await _adapterService.RegisterAsync(
                    new AdapterRegistrationDraft(
                        AdapterId.Trim(),
                        SelectedClient.Client,
                        ExecutablePath.Trim(),
                        ManagedRulesPath.Trim(),
                        MainConfigurationPath.Trim(),
                        EmptyToNull(DirectTarget)),
                    token);
                OperationMessage = result.Message;
                if (result.Succeeded)
                {
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
    }

    private async Task SyncAsync(
        AdapterInstallationItem? item,
        CancellationToken cancellationToken)
    {
        if (item is null)
        {
            return;
        }
        await RunAdapterOperationAsync(
            async token =>
            {
                var result = await _adapterService.SyncAsync(item.Id, token);
                PresentSyncResult(result);
                if (result.Succeeded)
                {
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
    }

    private async Task RemoveAsync(
        AdapterInstallationItem? item,
        CancellationToken cancellationToken)
    {
        if (item is null)
        {
            return;
        }
        await RunAdapterOperationAsync(
            async token =>
            {
                var result = await _adapterService.RemoveAsync(item.Id, token);
                OperationMessage = result.Message;
                if (result.Succeeded)
                {
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
    }

    private bool CanManage(AdapterInstallationItem? item)
    {
        return !IsBusy && item is not null;
    }

    private void PresentSyncResult(AdapterSyncResult result)
    {
        LastClientValidated = result.ClientValidated;
        LastConfigurationVerified = result.ConfigurationVerified;
        LastPathVerified = result.PathVerified;
        OnPropertyChanged(nameof(ClientValidationLabel));
        OnPropertyChanged(nameof(ConfigurationVerificationLabel));
        OnPropertyChanged(nameof(PathVerificationLabel));
        LastSnapshotLabel = result.SnapshotVersion == 0
            ? "活动快照：未取得"
            : $"活动快照：v{result.SnapshotVersion}";
        LastRuleCountLabel = $"规则：{result.RuleCount}";
        LastEvidenceMessage = result.Message;
        OperationMessage = result.Message;
        Blockers.Clear();
        foreach (var blocker in result.Blockers)
        {
            Blockers.Add(blocker);
        }
        OnPropertyChanged(nameof(HasBlockers));
    }

    private void RefreshManagedRulesSuggestion()
    {
        var current = ManagedRulesPath.Trim();
        if (current.Length > 0
            && !string.Equals(current, _suggestedManagedRulesPath, StringComparison.Ordinal))
        {
            return;
        }
        _suggestedManagedRulesPath =
            AdapterRegistrationValidator.SuggestManagedRulesPath(
                MainConfigurationPath,
                SelectedClient.ManagedFileName);
        ManagedRulesPath = _suggestedManagedRulesPath ?? string.Empty;
    }

    private void NotifyCollectionState()
    {
        OnPropertyChanged(nameof(HasItems));
        OnPropertyChanged(nameof(HasNoItems));
        SyncCommand.NotifyCanExecuteChanged();
        RemoveCommand.NotifyCanExecuteChanged();
    }

    private void InputChanged()
    {
        ValidationMessage = null;
        NotifyOperationCommands();
    }

    private async Task RunAdapterOperationAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        if (IsBusy)
        {
            return;
        }

        var task = RunOperationAsync(operation, cancellationToken);
        NotifyOperationCommands();
        try
        {
            await task;
        }
        finally
        {
            NotifyOperationCommands();
        }
    }

    private void NotifyOperationCommands()
    {
        ChooseExecutableCommand.NotifyCanExecuteChanged();
        ChooseConfigurationCommand.NotifyCanExecuteChanged();
        RegisterCommand.NotifyCanExecuteChanged();
        SyncCommand.NotifyCanExecuteChanged();
        RemoveCommand.NotifyCanExecuteChanged();
    }

    private static string? EmptyToNull(string value)
    {
        var trimmed = value.Trim();
        return trimmed.Length == 0 ? null : trimmed;
    }
}
