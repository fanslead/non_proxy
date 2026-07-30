using System.Collections.ObjectModel;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Activity;

public sealed class ActivityViewModel : LoadableViewModel
{
    private readonly IActivityService _activityService;

    public ActivityViewModel(IActivityService activityService)
        : base("活动记录")
    {
        _activityService = activityService;
    }

    public ObservableCollection<ActivityItem> Items { get; } = [];

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var items = await _activityService.GetRecentAsync(200, cancellationToken);
        Items.Clear();
        foreach (var item in items.OrderByDescending(item => item.Sequence))
        {
            Items.Add(item);
        }
    }
}
