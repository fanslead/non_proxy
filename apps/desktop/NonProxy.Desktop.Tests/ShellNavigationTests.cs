using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Shell;

namespace NonProxy.Desktop.Tests;

public sealed class ShellNavigationTests
{
    [Fact]
    public async Task InitializationLoadsDashboardAndNavigationChangesPage()
    {
        using var services = TestPlatformServices.Create();
        var shell = services.GetRequiredService<MainWindowViewModel>();

        await shell.InitializeCommand.ExecuteAsync(null);
        shell.SelectedNavigation = shell.NavigationItems.Single(item =>
            item.Label == "网站直连");

        Assert.Equal("网站直连", shell.CurrentPage.Title);
        Assert.Equal("等待控制服务", shell.Dashboard.State.StatusHeadline);
    }

    [Fact]
    public void NavigationLabelsCoverEveryProductArea()
    {
        using var services = TestPlatformServices.Create();
        var labels = services
            .GetRequiredService<MainWindowViewModel>()
            .NavigationItems
            .Select(item => item.Label)
            .ToArray();

        Assert.Equal(
            [
                "运行概览",
                "全部规则",
                "应用直连",
                "网站直连",
                "网络环境",
                "网络出口",
                "智能学习",
                "活动记录",
                "诊断",
                "设置",
            ],
            labels);
    }
}
