using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Applications;

public sealed partial class ApplicationsViewModel : LoadableViewModel
{
    private readonly IPolicyService _policyService;
    private readonly IApplicationCatalog _applicationCatalog;
    private IReadOnlyList<ApplicationCatalogEntry> _catalog = [];
    private HashSet<string> _configuredIdentities = new(StringComparer.Ordinal);

    [ObservableProperty]
    private string _searchText = string.Empty;

    [ObservableProperty]
    private string _catalogMessage = "正在读取可选择的应用…";

    [ObservableProperty]
    private bool _canChooseApplication;

    [ObservableProperty]
    private string? _operationMessage;

    public ApplicationsViewModel(
        IPolicyService policyService,
        IApplicationCatalog applicationCatalog)
        : base("应用直连")
    {
        _policyService = policyService;
        _applicationCatalog = applicationCatalog;
        AddCommand = new AsyncRelayCommand<ApplicationCatalogItem>(
            AddAsync,
            CanAdd);
        ChooseCommand = new AsyncRelayCommand(ChooseAsync, CanChoose);
        DeleteCommand = new AsyncRelayCommand<PolicyListItem>(
            DeleteAsync,
            CanDelete);
    }

    public ObservableCollection<ApplicationCatalogItem> AvailableApplications { get; } = [];

    public ObservableCollection<PolicyListItem> Items { get; } = [];

    public bool HasAvailableApplications => AvailableApplications.Count > 0;

    public bool HasNoAvailableApplications => !HasAvailableApplications;

    public IAsyncRelayCommand<ApplicationCatalogItem> AddCommand { get; }

    public IAsyncRelayCommand ChooseCommand { get; }

    public IAsyncRelayCommand<PolicyListItem> DeleteCommand { get; }

    partial void OnSearchTextChanged(string value)
    {
        RebuildAvailableApplications();
    }

    partial void OnCanChooseApplicationChanged(bool value)
    {
        ChooseCommand.NotifyCanExecuteChanged();
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var policiesTask = _policyService.GetCatalogAsync(cancellationToken);
        var applicationsTask = _applicationCatalog.ListAsync(cancellationToken);
        var applications = await applicationsTask;
        _catalog = applications.Applications;
        CatalogMessage = applications.Message;
        CanChooseApplication = applications.CanChooseApplication;
        RebuildAvailableApplications();

        var policies = await policiesTask;
        ReplacePolicies(policies.Items);
        RebuildAvailableApplications();
    }

    private bool CanAdd(ApplicationCatalogItem? item)
    {
        return !IsBusy && item is { IsConfigured: false };
    }

    private Task AddAsync(
        ApplicationCatalogItem? item,
        CancellationToken cancellationToken)
    {
        return item is null
            ? Task.CompletedTask
            : SaveApplicationAsync(item.Application, cancellationToken);
    }

    private bool CanChoose()
    {
        return !IsBusy && CanChooseApplication;
    }

    private Task ChooseAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                var selection = await _applicationCatalog.ChooseAsync(token);
                OperationMessage = selection.Message;
                if (selection.Succeeded && selection.Application is not null)
                {
                    await SaveApplicationCoreAsync(selection.Application, token);
                }
            },
            cancellationToken);
    }

    private Task SaveApplicationAsync(
        ApplicationCatalogEntry application,
        CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            token => SaveApplicationCoreAsync(application, token),
            cancellationToken);
    }

    private async Task SaveApplicationCoreAsync(
        ApplicationCatalogEntry application,
        CancellationToken cancellationToken)
    {
        var result = await _policyService.SaveAsync(
            new PolicyDraft(
                null,
                application.DisplayName,
                PolicyScope.Application,
                application.StableIdentity,
                PolicyAction.Direct,
                ApplicationSignerId: application.SignerIdentity,
                IncludeApplicationHelpers: application.IncludeHelpers),
            cancellationToken);

        OperationMessage = result.Message;
        if (result.Accepted)
        {
            await LoadCoreAsync(cancellationToken);
        }
    }

    private void RebuildAvailableApplications()
    {
        var query = SearchText.Trim();
        AvailableApplications.Clear();
        foreach (var application in _catalog
                     .Where(application => MatchesSearch(application, query))
                     .OrderByDescending(application => application.IsRunning)
                     .ThenBy(application => application.DisplayName))
        {
            AvailableApplications.Add(new ApplicationCatalogItem(
                application,
                _configuredIdentities.Contains(application.StableIdentity)));
        }
        OnPropertyChanged(nameof(HasAvailableApplications));
        OnPropertyChanged(nameof(HasNoAvailableApplications));
        AddCommand.NotifyCanExecuteChanged();
    }

    private static bool MatchesSearch(
        ApplicationCatalogEntry application,
        string query)
    {
        return query.Length == 0
            || application.DisplayName.Contains(
                query,
                StringComparison.CurrentCultureIgnoreCase)
            || (application.BundleIdentifier?.Contains(
                query,
                StringComparison.OrdinalIgnoreCase) ?? false);
    }

    private void ReplacePolicies(IEnumerable<PolicyListItem> items)
    {
        var policies = items
            .Where(item => item.Scope == PolicyScope.Application
                && item.Action == PolicyAction.Direct)
            .OrderBy(item => item.Name)
            .ToArray();
        Items.Clear();
        foreach (var item in policies)
        {
            Items.Add(item);
        }
        _configuredIdentities = policies
            .Select(item => item.MatchValue)
            .ToHashSet(StringComparer.Ordinal);
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
