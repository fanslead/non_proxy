using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Features.Shell;
using NonProxy.Desktop.Core.Views;

namespace NonProxy.Desktop.Tests;

public sealed class DashboardViewTests
{
    [AvaloniaFact]
    public void InitialStateRendersStatusHeadline()
    {
        using var services = TestPlatformServices.Create();
        var view = new DashboardView
        {
            DataContext = services.GetRequiredService<DashboardViewModel>(),
        };
        var window = new Window
        {
            Content = view,
        };

        try
        {
            window.Show();

            var headline = view.FindControl<TextBlock>("StatusHeadline");
            Assert.NotNull(headline);
            Assert.Equal("正在读取系统状态", headline.Text);
        }
        finally
        {
            window.Close();
        }
    }

    [AvaloniaFact]
    public void CompositionRootResolvesBoundMainWindow()
    {
        using var services = TestPlatformServices.Create();

        var window = services.GetRequiredService<MainWindow>();

        try
        {
            Assert.Same(
                services.GetRequiredService<MainWindowViewModel>(),
                window.DataContext);
        }
        finally
        {
            window.Close();
        }
    }
}
