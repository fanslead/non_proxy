using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Features.Shell;

public sealed class MainWindowViewModel
{
    public MainWindowViewModel(
        DashboardViewModel dashboard,
        IPlatformInformation platformInformation)
    {
        Dashboard = dashboard;
        PlatformLabel = platformInformation.DisplayName;
    }

    public DashboardViewModel Dashboard { get; }

    public string PlatformLabel { get; }
}
