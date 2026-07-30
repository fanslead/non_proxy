using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Settings;

public sealed partial class SettingsViewModel : LoadableViewModel
{
    private readonly IDesktopSettingsService _settingsService;

    [ObservableProperty]
    private string _theme = "System";

    [ObservableProperty]
    private bool _startAtLogin;

    [ObservableProperty]
    private bool _keepDataPlaneRunning = true;

    [ObservableProperty]
    private bool _collectDecisionHistory = true;

    [ObservableProperty]
    private string? _operationMessage;

    public SettingsViewModel(IDesktopSettingsService settingsService)
        : base("设置")
    {
        _settingsService = settingsService;
        SaveCommand = new AsyncRelayCommand(SaveAsync);
    }

    public IReadOnlyList<string> ThemeOptions { get; } =
        ["System", "Light", "Dark"];

    public IAsyncRelayCommand SaveCommand { get; }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var settings = await _settingsService.GetAsync(cancellationToken);
        Theme = settings.Theme;
        StartAtLogin = settings.StartAtLogin;
        KeepDataPlaneRunning = settings.KeepDataPlaneRunning;
        CollectDecisionHistory = settings.CollectDecisionHistory;
    }

    private Task SaveAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                await _settingsService.SaveAsync(
                    new DesktopSettings(
                        Theme,
                        StartAtLogin,
                        KeepDataPlaneRunning,
                        CollectDecisionHistory),
                    token);
                OperationMessage = "设置已保存。";
            },
            cancellationToken);
    }
}
