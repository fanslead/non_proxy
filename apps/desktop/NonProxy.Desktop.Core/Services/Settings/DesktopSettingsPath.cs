namespace NonProxy.Desktop.Core.Services.Settings;

public sealed class DesktopSettingsPath
{
    public DesktopSettingsPath(string filePath)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(filePath);
        FilePath = Path.GetFullPath(filePath);
    }

    public string FilePath { get; }

    public static DesktopSettingsPath ForCurrentUser()
    {
        var root = Environment.GetFolderPath(
            Environment.SpecialFolder.LocalApplicationData,
            Environment.SpecialFolderOption.Create);
        if (string.IsNullOrWhiteSpace(root))
        {
            throw new InvalidOperationException(
                "无法确定当前用户的本地应用数据目录。");
        }

        return new DesktopSettingsPath(
            Path.Combine(root, "NonProxy", "desktop-settings.json"));
    }
}
