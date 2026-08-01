using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Platform.Storage;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

internal sealed class AvaloniaWindowsExecutablePicker : IWindowsExecutablePicker
{
    private static readonly FilePickerFileType ExecutableFiles = new("Windows 应用")
    {
        Patterns = ["*.exe"],
    };

    public async Task<string?> PickAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var lifetime = Application.Current?.ApplicationLifetime
            as IClassicDesktopStyleApplicationLifetime;
        var provider = lifetime?.MainWindow?.StorageProvider;
        if (provider?.CanOpen != true)
        {
            return null;
        }

        var selected = await provider.OpenFilePickerAsync(
            new FilePickerOpenOptions
            {
                Title = "选择需要直连的 Windows 应用",
                AllowMultiple = false,
                FileTypeFilter = [ExecutableFiles],
            });
        cancellationToken.ThrowIfCancellationRequested();
        return selected.Count == 0 ? null : selected[0].TryGetLocalPath();
    }
}
