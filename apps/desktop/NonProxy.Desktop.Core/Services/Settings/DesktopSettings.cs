namespace NonProxy.Desktop.Core.Services.Settings;

public sealed record DesktopSettings(string Theme)
{
    public static DesktopSettings Defaults { get; } = new("System");
}
