using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Platform.Storage;
using NonProxy.Adapter.V1;

namespace NonProxy.Desktop.Core.Platform;

public sealed class AvaloniaAdapterFilePicker : IAdapterFilePicker
{
    private static readonly FilePickerFileType ExecutableFiles = new(
        "客户端或可执行文件")
    {
        Patterns = ["*.app", "*.exe", "*"],
        AppleUniformTypeIdentifiers =
        [
            "com.apple.application-bundle",
            "public.executable",
            "public.item",
        ],
    };

    public Task<AdapterFileSelection> PickExecutableAsync(
        AdapterClient client,
        string clientName,
        CancellationToken cancellationToken)
    {
        return PickAsync(
            $"选择 {clientName} 客户端或可执行文件",
            [ExecutableFiles],
            cancellationToken);
    }

    public Task<AdapterFileSelection> PickConfigurationAsync(
        AdapterClient client,
        string clientName,
        CancellationToken cancellationToken)
    {
        return PickAsync(
            $"选择 {clientName} 当前主配置",
            [ConfigurationFiles(client)],
            cancellationToken);
    }

    private static FilePickerFileType ConfigurationFiles(AdapterClient client)
    {
        return client switch
        {
            AdapterClient.Surge => new FilePickerFileType("Surge 配置")
            {
                Patterns = ["*.conf"],
                AppleUniformTypeIdentifiers = ["public.text"],
            },
            AdapterClient.Mihomo => new FilePickerFileType("YAML 配置")
            {
                Patterns = ["*.yaml", "*.yml"],
                AppleUniformTypeIdentifiers = ["public.yaml", "public.text"],
            },
            AdapterClient.SingBox => new FilePickerFileType("JSON / JSONC 配置")
            {
                Patterns = ["*.json", "*.jsonc"],
                AppleUniformTypeIdentifiers = ["public.json", "public.text"],
            },
            _ => new FilePickerFileType("配置文件")
            {
                Patterns = ["*"],
                AppleUniformTypeIdentifiers = ["public.item"],
            },
        };
    }

    private static async Task<AdapterFileSelection> PickAsync(
        string title,
        IReadOnlyList<FilePickerFileType> filters,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var lifetime = Application.Current?.ApplicationLifetime
            as IClassicDesktopStyleApplicationLifetime;
        var provider = lifetime?.MainWindow?.StorageProvider;
        if (provider?.CanOpen != true)
        {
            return AdapterFileSelection.Unavailable(
                "当前窗口无法打开系统文件选择器；未修改客户端登记。可继续手动粘贴绝对路径。");
        }

        var selected = await provider.OpenFilePickerAsync(
            new FilePickerOpenOptions
            {
                Title = title,
                AllowMultiple = false,
                FileTypeFilter = filters,
            });
        cancellationToken.ThrowIfCancellationRequested();
        if (selected.Count == 0)
        {
            return AdapterFileSelection.Cancelled;
        }

        var localPath = selected[0].TryGetLocalPath();
        return string.IsNullOrWhiteSpace(localPath)
            ? AdapterFileSelection.Unavailable(
                "所选项目没有可供本地适配器验证的文件路径；未修改客户端登记。")
            : AdapterFileSelection.Selected(localPath);
    }
}
