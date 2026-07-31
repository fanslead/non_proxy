using NonProxy.Desktop.Core.Features.Settings;
using NonProxy.Desktop.Core.Services.Settings;

namespace NonProxy.Desktop.Tests;

public sealed class SettingsViewModelTests
{
    [Fact]
    public async Task LoadAndSaveApplyOnlyPersistedTheme()
    {
        var settings = new RecordingSettingsService(new DesktopSettings("Dark"));
        var theme = new RecordingThemeService();
        var viewModel = new SettingsViewModel(settings, theme);

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal("Dark", viewModel.Theme);
        Assert.Equal(["Dark"], theme.Applied);

        viewModel.Theme = "Light";
        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.Equal(new DesktopSettings("Light"), settings.Saved);
        Assert.Equal(["Dark", "Light"], theme.Applied);
        Assert.Equal("设置已保存。", viewModel.OperationMessage);
        Assert.False(viewModel.HasError);
    }

    private sealed class RecordingSettingsService(DesktopSettings current)
        : IDesktopSettingsService
    {
        public DesktopSettings? Saved { get; private set; }

        public Task<DesktopSettings> GetAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(current);
        }

        public Task SaveAsync(
            DesktopSettings settings,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Saved = settings;
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingThemeService : IDesktopThemeService
    {
        public List<string> Applied { get; } = [];

        public void Apply(string theme)
        {
            Applied.Add(theme);
        }
    }
}
