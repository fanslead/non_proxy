using CommunityToolkit.Mvvm.ComponentModel;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Features.Dashboard;

public sealed partial class DashboardViewModel : ObservableObject
{
    private readonly IPlatformInformation _platformInformation;

    [ObservableProperty]
    private DashboardState _state = DashboardState.Initial;

    public DashboardViewModel(IPlatformInformation platformInformation)
    {
        _platformInformation = platformInformation;
    }

    public string PlatformLabel => _platformInformation.DisplayName;
}
