using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Validation;

namespace NonProxy.Desktop.Core.Features.Websites;

public sealed partial class WebsitesViewModel : LoadableViewModel
{
    private readonly IPolicyService _policyService;

    [ObservableProperty]
    private string _domain = string.Empty;

    [ObservableProperty]
    private string? _validationMessage;

    [ObservableProperty]
    private string? _operationMessage;

    public WebsitesViewModel(IPolicyService policyService)
        : base("网站直连")
    {
        _policyService = policyService;
        AddCommand = new AsyncRelayCommand(AddAsync, CanAdd);
        DeleteCommand = new AsyncRelayCommand<PolicyListItem>(
            DeleteAsync,
            CanDelete);
    }

    public ObservableCollection<PolicyListItem> Items { get; } = [];

    public IAsyncRelayCommand AddCommand { get; }

    public IAsyncRelayCommand<PolicyListItem> DeleteCommand { get; }

    partial void OnDomainChanged(string value)
    {
        ValidationMessage = null;
        AddCommand.NotifyCanExecuteChanged();
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var catalog = await _policyService.GetCatalogAsync(cancellationToken);
        Items.Clear();
        foreach (var item in catalog.Items
                     .Where(item => item.Scope == PolicyScope.Website)
                     .OrderBy(item => item.MatchValue))
        {
            Items.Add(item);
        }
    }

    protected override void OnBusyStateChanged()
    {
        AddCommand.NotifyCanExecuteChanged();
        DeleteCommand.NotifyCanExecuteChanged();
    }

    private bool CanAdd()
    {
        return !IsBusy && !string.IsNullOrWhiteSpace(Domain);
    }

    private Task AddAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                if (!DomainInputNormalizer.TryNormalize(
                        Domain,
                        out var normalized,
                        out var error))
                {
                    ValidationMessage = error;
                    return;
                }

                var result = await _policyService.SaveAsync(
                    new PolicyDraft(
                        null,
                        normalized,
                        PolicyScope.Website,
                        normalized,
                        PolicyAction.Direct),
                    token);

                OperationMessage = result.Message;
                if (result.Accepted)
                {
                    Domain = string.Empty;
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
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
