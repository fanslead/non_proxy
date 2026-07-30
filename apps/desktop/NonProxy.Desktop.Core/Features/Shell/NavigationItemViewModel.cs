using NonProxy.Desktop.Core.Features.Common;

namespace NonProxy.Desktop.Core.Features.Shell;

public sealed record NavigationItemViewModel(
    string Label,
    string Icon,
    IPageViewModel Page);
