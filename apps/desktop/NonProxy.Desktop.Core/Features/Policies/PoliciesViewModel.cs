using System.Collections.ObjectModel;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Policies;

public sealed partial class PoliciesViewModel : LoadableViewModel
{
    private readonly IPolicyService _policyService;

    public PoliciesViewModel(IPolicyService policyService)
        : base("全部规则")
    {
        _policyService = policyService;
    }

    public ObservableCollection<PolicyListItem> Items { get; } = [];

    public string ActiveSnapshotLabel { get; private set; } = "尚无已激活快照";

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var catalog = await _policyService.GetCatalogAsync(cancellationToken);

        Items.Clear();
        foreach (var item in catalog.Items.OrderByDescending(item => item.UpdatedAt))
        {
            Items.Add(item);
        }

        ActiveSnapshotLabel = catalog.ActiveSnapshotVersion is { } version
            ? $"当前生效：快照 v{version}"
            : "尚无已激活快照";
        OnPropertyChanged(nameof(ActiveSnapshotLabel));
    }
}
