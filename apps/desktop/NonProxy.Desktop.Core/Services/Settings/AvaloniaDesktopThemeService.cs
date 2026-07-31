using Avalonia;
using Avalonia.Styling;

namespace NonProxy.Desktop.Core.Services.Settings;

public sealed class AvaloniaDesktopThemeService : IDesktopThemeService
{
    public void Apply(string theme)
    {
        var application = Application.Current;
        if (application is null)
        {
            return;
        }

        application.RequestedThemeVariant = theme switch
        {
            "Light" => ThemeVariant.Light,
            "Dark" => ThemeVariant.Dark,
            _ => ThemeVariant.Default,
        };
    }
}
