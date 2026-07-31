using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Services.Settings;

namespace NonProxy.Desktop.Core.Bootstrap;

public sealed class DesktopLifetimeController : IDisposable
{
    private readonly IDesktopSettingsService _settingsService;
    private readonly IDesktopThemeService _themeService;
    private IClassicDesktopStyleApplicationLifetime? _lifetime;
    private IActivatableLifetime? _activatableLifetime;
    private Window? _window;
    private TrayIcon? _trayIcon;
    private bool _isQuitting;
    private bool _disposed;

    public DesktopLifetimeController(
        IDesktopSettingsService settingsService,
        IDesktopThemeService themeService)
    {
        _settingsService = settingsService;
        _themeService = themeService;
    }

    public void Attach(
        Application application,
        IClassicDesktopStyleApplicationLifetime lifetime,
        Window window)
    {
        ArgumentNullException.ThrowIfNull(application);
        ArgumentNullException.ThrowIfNull(lifetime);
        ArgumentNullException.ThrowIfNull(window);
        ObjectDisposedException.ThrowIf(_disposed, this);
        if (_lifetime is not null)
        {
            throw new InvalidOperationException("桌面生命周期已经初始化。");
        }

        _lifetime = lifetime;
        _window = window;
        lifetime.ShutdownMode = ShutdownMode.OnExplicitShutdown;
        lifetime.ShutdownRequested += OnShutdownRequested;
        lifetime.Exit += OnExit;
        window.Closing += OnWindowClosing;

        if (lifetime is IActivatableLifetime activatableLifetime)
        {
            _activatableLifetime = activatableLifetime;
            activatableLifetime.Activated += OnActivated;
        }

        var showCommand = new RelayCommand(ShowWindow);
        var quitCommand = new RelayCommand(QuitInterface);
        _trayIcon = new TrayIcon
        {
            Command = showCommand,
            Icon = TrayIconImage.Create(),
            IsVisible = true,
            Menu = CreateActionMenu(showCommand, quitCommand),
            ToolTipText = "NonProxy · 直连策略中心",
        };
        TrayIcon.SetIcons(application, new TrayIcons { _trayIcon });
        NativeMenu.SetMenu(
            application,
            new NativeMenu
            {
                Items =
                {
                    new NativeMenuItem
                    {
                        Header = "NonProxy",
                        Menu = CreateActionMenu(showCommand, quitCommand),
                    },
                },
            });
        NativeDock.SetMenu(
            application,
            CreateActionMenu(showCommand, quitCommand));
        _ = ApplySavedThemeAsync();
    }

    internal static NativeMenu CreateActionMenu(
        System.Windows.Input.ICommand showCommand,
        System.Windows.Input.ICommand quitCommand)
    {
        return new NativeMenu
        {
            Items =
            {
                new NativeMenuItem
                {
                    Header = "显示 NonProxy",
                    Command = showCommand,
                },
                new NativeMenuItemSeparator(),
                new NativeMenuItem
                {
                    Header = "退出 NonProxy 界面",
                    Command = quitCommand,
                },
            },
        };
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        if (_window is not null)
        {
            _window.Closing -= OnWindowClosing;
        }

        if (_lifetime is not null)
        {
            _lifetime.ShutdownRequested -= OnShutdownRequested;
            _lifetime.Exit -= OnExit;
        }

        if (_activatableLifetime is not null)
        {
            _activatableLifetime.Activated -= OnActivated;
        }

        _trayIcon?.Dispose();
        _trayIcon = null;
    }

    private async Task ApplySavedThemeAsync()
    {
        try
        {
            var settings = await _settingsService.GetAsync(CancellationToken.None);
            _themeService.Apply(settings.Theme);
        }
        catch (Exception)
        {
            _themeService.Apply(DesktopSettings.Defaults.Theme);
        }
    }

    private void OnWindowClosing(object? sender, WindowClosingEventArgs eventArgs)
    {
        if (_isQuitting)
        {
            return;
        }

        eventArgs.Cancel = true;
        _window?.Hide();
    }

    private void OnShutdownRequested(
        object? sender,
        ShutdownRequestedEventArgs eventArgs)
    {
        _isQuitting = true;
    }

    private void OnExit(
        object? sender,
        ControlledApplicationLifetimeExitEventArgs eventArgs)
    {
        Dispose();
    }

    private void OnActivated(object? sender, ActivatedEventArgs eventArgs)
    {
        if (eventArgs.Kind == ActivationKind.Reopen)
        {
            ShowWindow();
        }
    }

    private void ShowWindow()
    {
        if (_window is null)
        {
            return;
        }

        if (!_window.IsVisible)
        {
            _window.Show();
        }

        if (_window.WindowState == WindowState.Minimized)
        {
            _window.WindowState = WindowState.Normal;
        }

        _window.Activate();
    }

    private void QuitInterface()
    {
        if (_lifetime is null)
        {
            return;
        }

        _isQuitting = true;
        if (!_lifetime.TryShutdown())
        {
            _isQuitting = false;
        }
    }
}
