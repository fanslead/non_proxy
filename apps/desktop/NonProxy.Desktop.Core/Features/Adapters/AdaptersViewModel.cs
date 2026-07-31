using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Adapter.V1;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Adapters;

namespace NonProxy.Desktop.Core.Features.Adapters;

public sealed partial class AdaptersViewModel : LoadableViewModel
{
    private readonly IAdapterManagementService _adapterService;
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

    public AdaptersViewModel(IAdapterManagementService adapterService)
        : base("客户端协同")
    {
        _adapterService = adapterService;
        RegisterCommand = new AsyncRelayCommand(RegisterAsync, CanRegister);
        SyncCommand = new AsyncRelayCommand<AdapterInstallationItem>(
            SyncAsync,
            CanManage);
        RemoveCommand = new AsyncRelayCommand<AdapterInstallationItem>(
            RemoveAsync,
            CanManage);
    }

    public static IReadOnlyList<AdapterClientOption> ClientOptions { get; } =
    [
        new(
            "Surge for Mac",
            AdapterClient.Surge,
            "surge-primary",
            "nonproxy.list",
            "选择 Surge.app 内的 surge-cli 可执行文件。",
            "选择当前正在使用的 Surge 配置文件。",
            "Surge 固定使用 DIRECT，通常留空即可。"),
        new(
            "Clash / Mihomo",
            AdapterClient.Mihomo,
            "mihomo-primary",
            "nonproxy.yaml",
            "选择当前客户端实际运行的 Mihomo 可执行文件。",
            "选择包含 external-controller 的当前主配置。",
            "Mihomo 固定使用 DIRECT，通常留空即可。"),
        new(
            "sing-box",
            AdapterClient.SingBox,
            "sing-box-primary",
            "nonproxy.json",
            "选择当前客户端实际运行的 sing-box 可执行文件。",
            "选择唯一运行进程正在使用的主配置。",
            "存在多个 direct outbound 时，填写要使用的 outbound tag。"),
    ];

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
