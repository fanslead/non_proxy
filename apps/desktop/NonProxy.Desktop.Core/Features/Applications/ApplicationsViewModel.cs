using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Applications;

public sealed partial class ApplicationsViewModel : LoadableViewModel
{
    private readonly IPolicyService _policyService;

    [ObservableProperty]
    private string _applicationName = string.Empty;

    [ObservableProperty]
    private string _applicationIdentity = string.Empty;

    [ObservableProperty]
    private string? _operationMessage;

    public ApplicationsViewModel(IPolicyService policyService)
        : base("应用直连")
    {
        _policyService = policyService;
        AddCommand = new AsyncRelayCommand(AddAsync, CanAdd);
    }

    public ObservableCollection<PolicyListItem> Items { get; } = [];

    public IAsyncRelayCommand AddCommand { get; }

    partial void OnApplicationNameChanged(string value)
    {
        AddCommand.NotifyCanExecuteChanged();
    }

    partial void OnApplicationIdentityChanged(string value)
    {
        AddCommand.NotifyCanExecuteChanged();
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var catalog = await _policyService.GetCatalogAsync(cancellationToken);
        ReplaceItems(catalog.Items.Where(item =>
            item.Scope is PolicyScope.Application
                or PolicyScope.ApplicationAndDestination));
    }

    private bool CanAdd()
    {
        return !IsBusy
            && !string.IsNullOrWhiteSpace(ApplicationName)
            && !string.IsNullOrWhiteSpace(ApplicationIdentity);
    }

    private Task AddAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                var result = await _policyService.SaveAsync(
                    new PolicyDraft(
                        null,
                        ApplicationName.Trim(),
                        PolicyScope.Application,
                        ApplicationIdentity.Trim(),
                        PolicyAction.Direct),
                    token);

                OperationMessage = result.Message;
                if (result.Accepted)
                {
                    ApplicationName = string.Empty;
                    ApplicationIdentity = string.Empty;
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
    }

    private void ReplaceItems(IEnumerable<PolicyListItem> items)
    {
        Items.Clear();
        foreach (var item in items.OrderBy(item => item.Name))
        {
            Items.Add(item);
        }
    }
}
