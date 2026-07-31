using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Settings;

namespace NonProxy.Desktop.Core.Features.Settings;

public sealed partial class SettingsViewModel : LoadableViewModel
{
    private readonly IDesktopSettingsService _settingsService;
    private readonly IDesktopThemeService _themeService;

    [ObservableProperty]
    private string _theme = "System";

    [ObservableProperty]
    private string? _operationMessage;

    public SettingsViewModel(
        IDesktopSettingsService settingsService,
        IDesktopThemeService themeService)
        : base("设置")
    {
        _settingsService = settingsService;
        _themeService = themeService;
        SaveCommand = new AsyncRelayCommand(SaveAsync);
    }

    public IReadOnlyList<string> ThemeOptions { get; } =
        ["System", "Light", "Dark"];

    public IAsyncRelayCommand SaveCommand { get; }

    partial void OnThemeChanged(string value)
    {
        OperationMessage = null;
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var settings = await _settingsService.GetAsync(cancellationToken);
        Theme = settings.Theme;
        _themeService.Apply(settings.Theme);
    }

    private Task SaveAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                await _settingsService.SaveAsync(
                    new DesktopSettings(Theme),
                    token);
                _themeService.Apply(Theme);
                OperationMessage = "设置已保存。";
            },
            cancellationToken);
    }
}
