using System.Collections.ObjectModel;
using System.Text;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Networks;

public sealed partial class NetworkProfilesViewModel : LoadableViewModel
{
    private readonly ICurrentNetworkEnvironment _networkEnvironment;
    private readonly INetworkProfileService _profileService;
    private readonly IPolicyService _policyService;
    private CurrentNetworkEnvironment? _detectedNetwork;
    private NetworkProfileCatalog _profileCatalog = NetworkProfileCatalog.Empty;
    private PolicyCatalog _policyCatalog = PolicyCatalog.Empty;

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(SaveDirectCommand))]
    private string _networkName = string.Empty;

    [ObservableProperty]
    private string? _validationMessage;

    [ObservableProperty]
    private string? _detectionMessage;

    [ObservableProperty]
    private string? _operationMessage;

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(SaveDirectCommand))]
    private bool _hasDetectedNetwork;

    [ObservableProperty]
    private string _detectedFingerprintLabel = string.Empty;

    [ObservableProperty]
    private string _detectedFingerprintPreview = string.Empty;

    [ObservableProperty]
    private string _detectedAccuracyLabel = string.Empty;

    public NetworkProfilesViewModel(
        ICurrentNetworkEnvironment networkEnvironment,
        INetworkProfileService profileService,
        IPolicyService policyService)
        : base("网络环境")
    {
        _networkEnvironment = networkEnvironment;
        _profileService = profileService;
        _policyService = policyService;
        DetectCommand = new AsyncRelayCommand(DetectAsync, () => !IsBusy);
        SaveDirectCommand = new AsyncRelayCommand(SaveDirectAsync, CanSaveDirect);
        DeleteCommand = new AsyncRelayCommand<NetworkProfileViewItem>(
            DeleteAsync,
            item => !IsBusy && item is not null);
    }

    public ObservableCollection<NetworkProfileViewItem> Items { get; } = [];

    public IAsyncRelayCommand DetectCommand { get; }

    public IAsyncRelayCommand SaveDirectCommand { get; }

    public IAsyncRelayCommand<NetworkProfileViewItem> DeleteCommand { get; }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var profilesTask = _profileService.GetCatalogAsync(cancellationToken);
        var policiesTask = _policyService.GetCatalogAsync(cancellationToken);
        await Task.WhenAll(profilesTask, policiesTask);
        _profileCatalog = await profilesTask;
        _policyCatalog = await policiesTask;
        RebuildItems();
    }

    private Task DetectAsync(CancellationToken cancellationToken)
    {
        return RunNetworkOperationAsync(
            async token =>
            {
                ValidationMessage = null;
                OperationMessage = null;
                var detected = await _networkEnvironment.CaptureAsync(token);
                if (!detected.IsAvailable
                    || detected.FingerprintKind is null
                    || string.IsNullOrWhiteSpace(detected.FingerprintValue))
                {
                    ClearDetection();
                    DetectionMessage = detected.Message;
                    return;
                }

                _detectedNetwork = detected;
                HasDetectedNetwork = true;
                DetectionMessage = detected.Message;
                SetDetectedFingerprintLabels(detected);
                await LoadCoreAsync(token);
                RefreshDetectedProfileName();
            },
            cancellationToken);
    }

    private bool CanSaveDirect()
    {
        return !IsBusy
            && HasDetectedNetwork
            && !string.IsNullOrWhiteSpace(NetworkName);
    }

    private Task SaveDirectAsync(CancellationToken cancellationToken)
    {
        return RunNetworkOperationAsync(
            async token =>
            {
                if (_detectedNetwork?.FingerprintKind is not { } kind
                    || string.IsNullOrWhiteSpace(_detectedNetwork.FingerprintValue))
                {
                    OperationMessage = "请先检测当前网络。";
                    return;
                }

                if (!ValidateDisplayName(NetworkName))
                {
                    return;
                }

                await LoadCoreAsync(token);
                var existing = FindDetectedProfile();
                var editablePolicies = existing is null
                    ? Array.Empty<PolicyListItem>()
                    : EditablePolicies(existing.Id);
                if (editablePolicies.Length > 1)
                {
                    OperationMessage = "当前网络存在多条可编辑规则，请先在“全部规则”中清理冲突。";
                    return;
                }

                var created = false;
                var profileSaved = false;
                var profile = existing;
                if (profile is null
                    || !string.Equals(
                        profile.DisplayName,
                        NetworkName.Trim(),
                        StringComparison.Ordinal))
                {
                    var saved = await _profileService.SaveAsync(
                        new NetworkProfileDraft(
                            profile?.Id,
                            NetworkName,
                            kind,
                            _detectedNetwork.FingerprintValue,
                            profile?.Revision),
                        token);
                    if (!saved.Accepted || saved.Profile is null)
                    {
                        OperationMessage = saved.Message;
                        return;
                    }

                    created = profile is null;
                    profileSaved = true;
                    profile = saved.Profile;
                }

                var editablePolicy = editablePolicies.SingleOrDefault();
                if (editablePolicy is
                    {
                        Action: PolicyAction.Direct,
                        State: PolicyApplyState.Active,
                    })
                {
                    OperationMessage = "该网络的直连规则已经处于已生效状态。";
                    await LoadCoreAsync(token);
                    return;
                }

                if (editablePolicy is
                    {
                        Action: PolicyAction.Direct,
                        State: PolicyApplyState.Pending,
                    })
                {
                    OperationMessage = "该网络的直连规则正在等待系统组件确认。";
                    await LoadCoreAsync(token);
                    return;
                }

                ApplyResult policyResult;
                try
                {
                    policyResult = await _policyService.SaveAsync(
                        new PolicyDraft(
                            editablePolicy?.Id,
                            $"{profile.DisplayName}直连",
                            PolicyScope.Network,
                            profile.Id,
                            PolicyAction.Direct,
                            editablePolicy?.Revision),
                        token);
                }
                catch (ControlServiceException exception)
                {
                    OperationMessage = profileSaved
                        ? $"网络配置已保存，但无法确认规则操作结果：{exception.UserMessage}"
                        : $"网络配置未改变，但无法确认规则操作结果：{exception.UserMessage}";
                    await LoadCoreAsync(token);
                    return;
                }
                if (!policyResult.Accepted && created)
                {
                    var rollback = await _profileService.DeleteAsync(
                        profile.Id,
                        profile.Revision,
                        token);
                    OperationMessage = rollback.Accepted
                        ? $"{policyResult.Message} 已回收本次新建的网络配置。"
                        : $"{policyResult.Message} 网络配置回收失败：{rollback.Message}";
                }
                else
                {
                    OperationMessage = policyResult.Message;
                }

                await LoadCoreAsync(token);
            },
            cancellationToken);
    }

    private Task DeleteAsync(
        NetworkProfileViewItem? item,
        CancellationToken cancellationToken)
    {
        if (item is null)
        {
            return Task.CompletedTask;
        }

        return RunNetworkOperationAsync(
            async token =>
            {
                await LoadCoreAsync(token);
                var profile = _profileCatalog.Items.FirstOrDefault(candidate =>
                    string.Equals(candidate.Id, item.Id, StringComparison.Ordinal));
                if (profile is null)
                {
                    OperationMessage = "网络配置已经不存在。";
                    return;
                }

                foreach (var policy in MatchingPolicies(profile.Id))
                {
                    var deletedPolicy = await _policyService.DeleteAsync(
                        policy.Id,
                        token);
                    if (!deletedPolicy.Accepted)
                    {
                        OperationMessage = $"直连规则未能删除：{deletedPolicy.Message}";
                        await LoadCoreAsync(token);
                        return;
                    }
                }

                var deleted = await _profileService.DeleteAsync(
                    profile.Id,
                    profile.Revision,
                    token);
                OperationMessage = deleted.Accepted
                    ? "网络配置及其规则草稿已删除；旧快照可能仍在等待系统组件移除。"
                    : deleted.Message;
                await LoadCoreAsync(token);
            },
            cancellationToken);
    }

    private void RebuildItems()
    {
        Items.Clear();
        foreach (var profile in _profileCatalog.Items.OrderBy(item => item.DisplayName))
        {
            Items.Add(new NetworkProfileViewItem(
                profile,
                MatchingPolicies(profile.Id)));
        }
    }

    private PolicyListItem[] MatchingPolicies(string profileId)
    {
        return _policyCatalog.Items
            .Where(policy => policy.Scope == PolicyScope.Network
                && string.Equals(
                    policy.MatchValue,
                    profileId,
                    StringComparison.Ordinal))
            .ToArray();
    }

    private PolicyListItem[] EditablePolicies(string profileId)
    {
        return MatchingPolicies(profileId)
            .Where(policy => policy.State != PolicyApplyState.PendingRemoval)
            .ToArray();
    }

    private NetworkProfileListItem? FindDetectedProfile()
    {
        return _profileCatalog.Items.SingleOrDefault(profile =>
            profile.FingerprintKind == _detectedNetwork?.FingerprintKind
            && string.Equals(
                profile.FingerprintValue,
                _detectedNetwork?.FingerprintValue,
                StringComparison.Ordinal));
    }

    private void RefreshDetectedProfileName()
    {
        if (_detectedNetwork is null)
        {
            return;
        }

        var existing = FindDetectedProfile();
        NetworkName = existing?.DisplayName ?? _detectedNetwork.SuggestedName;
    }

    private void SetDetectedFingerprintLabels(CurrentNetworkEnvironment detected)
    {
        var item = new NetworkProfileListItem(
            "detected",
            detected.SuggestedName,
            detected.FingerprintKind!.Value,
            detected.FingerprintValue!,
            1);
        DetectedFingerprintLabel = item.FingerprintKindLabel;
        DetectedFingerprintPreview = item.FingerprintPreview;
        DetectedAccuracyLabel = item.AccuracyLabel;
    }

    private bool ValidateDisplayName(string value)
    {
        var normalized = value.Trim();
        if (string.IsNullOrEmpty(normalized)
            || Encoding.UTF8.GetByteCount(normalized) > 128
            || normalized.Any(char.IsControl))
        {
            ValidationMessage = "请输入不超过 128 个 UTF-8 字节的网络名称。";
            return false;
        }

        ValidationMessage = null;
        return true;
    }

    private void ClearDetection()
    {
        _detectedNetwork = null;
        HasDetectedNetwork = false;
        NetworkName = string.Empty;
        DetectedFingerprintLabel = string.Empty;
        DetectedFingerprintPreview = string.Empty;
        DetectedAccuracyLabel = string.Empty;
    }

    private async Task RunNetworkOperationAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        if (IsBusy)
        {
            return;
        }

        var task = RunOperationAsync(operation, cancellationToken);
        NotifyCommandStates();
        try
        {
            await task;
        }
        finally
        {
            NotifyCommandStates();
        }
    }

    private void NotifyCommandStates()
    {
        DetectCommand.NotifyCanExecuteChanged();
        SaveDirectCommand.NotifyCanExecuteChanged();
        DeleteCommand.NotifyCanExecuteChanged();
    }
}
