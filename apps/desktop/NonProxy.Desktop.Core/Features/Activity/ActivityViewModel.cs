using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Activity;

public sealed partial class ActivityViewModel : LoadableViewModel
{
    private readonly IActivityService _activityService;
    private readonly IPolicyService _policyService;
    private readonly IPlatformInformation _platformInformation;

    [ObservableProperty]
    private string? _operationMessage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDirectConfirmationVisible))]
    private ActivityDirectConfirmation? _directConfirmation;

    public ActivityViewModel(
        IActivityService activityService,
        IPolicyService policyService,
        IPlatformInformation platformInformation)
        : base("活动记录")
    {
        _activityService = activityService;
        _policyService = policyService;
        _platformInformation = platformInformation;
        RequestDirectCommand = new RelayCommand<ActivityRecordViewItem>(
            RequestDirect,
            CanRequestDirect);
        CancelDirectCommand = new RelayCommand(CancelDirect);
        ConfirmDirectCommand = new AsyncRelayCommand(
            ConfirmDirectAsync,
            () => DirectConfirmation is not null);
    }

    public ObservableCollection<ActivityRecordViewItem> Items { get; } = [];

    public bool HasItems => Items.Count > 0;

    public bool HasNoItems => !HasItems;

    public bool IsDirectConfirmationVisible => DirectConfirmation is not null;

    public IRelayCommand<ActivityRecordViewItem> RequestDirectCommand { get; }

    public IRelayCommand CancelDirectCommand { get; }

    public IAsyncRelayCommand ConfirmDirectCommand { get; }

    partial void OnDirectConfirmationChanged(ActivityDirectConfirmation? value)
    {
        ConfirmDirectCommand.NotifyCanExecuteChanged();
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        DirectConfirmation = null;
        var activityTask = _activityService.GetRecentAsync(200, cancellationToken);
        var policiesTask = _policyService.GetCatalogAsync(cancellationToken);
        await Task.WhenAll(activityTask, policiesTask);

        var configuredApplicationIdentities = policiesTask.Result.Items
            .Where(item => item.Scope is PolicyScope.Application
                or PolicyScope.ApplicationAndDestination)
            .Select(item => item.MatchValue)
            .ToHashSet(StringComparer.Ordinal);

        Items.Clear();
        foreach (var item in activityTask.Result
                     .OrderByDescending(item => item.Sequence))
        {
            Items.Add(ActivityRecordViewItem.Create(
                item,
                _platformInformation.Platform,
                configuredApplicationIdentities));
        }
        OnPropertyChanged(nameof(HasItems));
        OnPropertyChanged(nameof(HasNoItems));
    }

    private static bool CanRequestDirect(ActivityRecordViewItem? item)
    {
        return item is { CanPrepareDirect: true };
    }

    private void RequestDirect(ActivityRecordViewItem? item)
    {
        if (item is not { CanPrepareDirect: true }
            || item.Record.ApplicationSignerId is not { } signerId)
        {
            return;
        }

        OperationMessage = null;
        DirectConfirmation = new ActivityDirectConfirmation(
            item.Application,
            item.Record.ApplicationRuleStableId,
            signerId,
            item.Record.ApplicationPlatform == PlatformKind.MacOS);
    }

    private void CancelDirect()
    {
        DirectConfirmation = null;
    }

    private Task ConfirmDirectAsync(CancellationToken cancellationToken)
    {
        var confirmation = DirectConfirmation;
        if (confirmation is null)
        {
            return Task.CompletedTask;
        }

        return RunOperationAsync(
            async token =>
            {
                var result = await _policyService.SaveAsync(
                    new PolicyDraft(
                        null,
                        confirmation.Application,
                        PolicyScope.Application,
                        confirmation.RuleStableId,
                        PolicyAction.Direct,
                        ApplicationSignerId: confirmation.SignerId,
                        IncludeApplicationHelpers: confirmation.IncludeHelpers),
                    token);
                OperationMessage = result.Message;
                if (result.Accepted)
                {
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
    }
}
