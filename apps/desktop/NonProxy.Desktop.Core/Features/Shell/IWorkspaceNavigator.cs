namespace NonProxy.Desktop.Core.Features.Shell;

public enum WorkspaceDestination
{
    Policies,
    Applications,
    Websites,
    Outbounds,
    Adapters,
    Activity,
}

public interface IWorkspaceNavigator
{
    event Action<WorkspaceDestination>? NavigationRequested;

    void NavigateTo(WorkspaceDestination destination);
}

public sealed class WorkspaceNavigator : IWorkspaceNavigator
{
    public event Action<WorkspaceDestination>? NavigationRequested;

    public void NavigateTo(WorkspaceDestination destination)
    {
        if (!Enum.IsDefined(destination))
        {
            throw new ArgumentOutOfRangeException(nameof(destination));
        }

        NavigationRequested?.Invoke(destination);
    }
}
