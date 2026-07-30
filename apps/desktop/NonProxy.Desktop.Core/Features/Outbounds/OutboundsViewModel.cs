using System.Collections.ObjectModel;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed class OutboundsViewModel : LoadableViewModel
{
    private readonly IOutboundService _outboundService;

    public OutboundsViewModel(IOutboundService outboundService)
        : base("网络出口")
    {
        _outboundService = outboundService;
    }

    public ObservableCollection<OutboundListItem> Items { get; } = [];

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var items = await _outboundService.ListAsync(cancellationToken);
        Items.Clear();
        foreach (var item in items.OrderBy(item => item.Name))
        {
            Items.Add(item);
        }
    }
}
