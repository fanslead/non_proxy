using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Bootstrap;
using NonProxy.Desktop.Core.Features.Learning;
using NonProxy.Desktop.Core.Features.Settings;
using NonProxy.Desktop.Core.Services.Settings;

namespace NonProxy.Desktop.Tests;

public sealed class DesktopExperienceViewTests
{
    [AvaloniaFact]
    public void LearningPageExplainsRealExtensionFlowWithoutFakeControls()
    {
        using var services = TestPlatformServices.Create();
        var view = new LearningView
        {
            DataContext = services.GetRequiredService<LearningViewModel>(),
        };
        var window = new Window { Content = view };

        try
        {
            window.Show();
            var text = view.GetLogicalDescendants()
                .OfType<TextBlock>()
                .Select(value => value.Text)
                .ToArray();
            var buttons = view.GetLogicalDescendants().OfType<Button>().ToArray();

            Assert.Contains(text, value =>
                value?.Contains("点击 NonProxy 浏览器扩展", StringComparison.Ordinal)
                    == true);
            Assert.Contains(text, value =>
                value?.Contains("应用不需要学习地址", StringComparison.Ordinal)
                    == true);
            Assert.Empty(buttons);
        }
        finally
        {
            window.Close();
        }
    }

    [AvaloniaFact]
    public void SettingsPageOnlyOffersImplementedThemeMutation()
    {
        using var services = TestPlatformServices.Create();
        var view = new SettingsView
        {
            DataContext = services.GetRequiredService<SettingsViewModel>(),
        };
        var window = new Window { Content = view };

        try
        {
            window.Show();
            var names = view.GetLogicalDescendants()
                .OfType<Button>()
                .Select(AutomationProperties.GetName)
                .ToArray();

            Assert.Equal("保存桌面设置", Assert.Single(names));
            Assert.Empty(view.GetLogicalDescendants().OfType<CheckBox>());
        }
        finally
        {
            window.Close();
        }
    }

    [AvaloniaFact]
    public void TrayAssetsExposeRestoreAndExplicitUiExit()
    {
        var icon = TrayIconImage.Create();
        var show = new CommunityToolkit.Mvvm.Input.RelayCommand(() => { });
        var pause = new CommunityToolkit.Mvvm.Input.RelayCommand(() => { });
        var direct = new CommunityToolkit.Mvvm.Input.RelayCommand(() => { });
        var proxy = new CommunityToolkit.Mvvm.Input.RelayCommand(() => { });
        var quit = new CommunityToolkit.Mvvm.Input.RelayCommand(() => { });
        var menu = DesktopLifetimeController.CreateActionMenu(
            show,
            pause,
            direct,
            proxy,
            quit);

        Assert.NotNull(icon);
        Assert.Collection(
            menu.Items,
            item => Assert.Equal("显示 NonProxy", Assert.IsType<NativeMenuItem>(item).Header),
            item => Assert.IsType<NativeMenuItemSeparator>(item),
            item => Assert.Equal("暂停 5 分钟…", Assert.IsType<NativeMenuItem>(item).Header),
            item => Assert.Equal("全部直连 5 分钟…", Assert.IsType<NativeMenuItem>(item).Header),
            item => Assert.Equal("全部代理 5 分钟…", Assert.IsType<NativeMenuItem>(item).Header),
            item => Assert.IsType<NativeMenuItemSeparator>(item),
            item => Assert.Equal(
                "退出 NonProxy 界面",
                Assert.IsType<NativeMenuItem>(item).Header));
    }

    [AvaloniaFact]
    public void NormalWindowCloseHidesUiAndKeepsExplicitLifetime()
    {
        var application = Assert.IsAssignableFrom<Avalonia.Application>(
            Avalonia.Application.Current);
        using var lifetime = new ClassicDesktopStyleApplicationLifetime();
        var window = new Window();
        lifetime.MainWindow = window;
        using var controller = new DesktopLifetimeController(
            new DefaultSettingsService(),
            new NoOpThemeService());
        controller.Attach(application, lifetime, window);

        window.Show();
        Assert.True(window.IsVisible);

        window.Close();

        Assert.False(window.IsVisible);
        Assert.Equal(ShutdownMode.OnExplicitShutdown, lifetime.ShutdownMode);
        controller.Dispose();
        window.Close();
    }

    private sealed class DefaultSettingsService : IDesktopSettingsService
    {
        public Task<DesktopSettings> GetAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(DesktopSettings.Defaults);
        }

        public Task SaveAsync(
            DesktopSettings settings,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }
    }

    private sealed class NoOpThemeService : IDesktopThemeService
    {
        public void Apply(string theme)
        {
        }
    }
}
