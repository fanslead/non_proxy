using NonProxy.Adapter.V1;

namespace NonProxy.Desktop.Core.Platform;

public sealed record AdapterFileSelection(
    string? LocalPath,
    string? Message)
{
    public bool IsSelected => !string.IsNullOrWhiteSpace(LocalPath);

    public static AdapterFileSelection Selected(string localPath)
    {
        return new AdapterFileSelection(localPath, null);
    }

    public static AdapterFileSelection Cancelled { get; } = new(null, null);

    public static AdapterFileSelection Unavailable(string message)
    {
        return new AdapterFileSelection(null, message);
    }
}

public interface IAdapterFilePicker
{
    Task<AdapterFileSelection> PickExecutableAsync(
        AdapterClient client,
        string clientName,
        CancellationToken cancellationToken);

    Task<AdapterFileSelection> PickConfigurationAsync(
        AdapterClient client,
        string clientName,
        CancellationToken cancellationToken);
}
