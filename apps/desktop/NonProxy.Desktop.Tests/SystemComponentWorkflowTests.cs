using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class SystemComponentWorkflowTests
{
    [Fact]
    public async Task ApprovalStateOffersDirectSystemSettingsRecovery()
    {
        var installer = new RecordingSystemComponentInstaller
        {
            State = ComponentState(
                SystemComponentStatus.AwaitingApproval,
                SystemComponentStepStatus.AwaitingApproval,
                canOpenSystemSettings: true),
        };
        using var services = TestPlatformServices.Create(
            configure: registrations =>
                registrations.AddSingleton<ISystemComponentInstaller>(
                    installer));
        var viewModel = services.GetRequiredService<DashboardViewModel>();

        await viewModel.RefreshCommand.ExecuteAsync(null);
        await viewModel.OpenSystemSettingsCommand.ExecuteAsync(null);

        Assert.Equal("等待系统授权", viewModel.State.ComponentLabel);
        Assert.Equal("我已允许，重新检查", viewModel.State.ComponentActionLabel);
        Assert.True(viewModel.State.Component.CanOpenSystemSettings);
        Assert.Equal(4, viewModel.State.Component.Steps.Count);
        Assert.Equal(1, installer.OpenSettingsCalls);
        Assert.Contains(
            "已打开",
            viewModel.OperationMessage,
            StringComparison.Ordinal);
    }

    [Fact]
    public async Task ApprovalRequiredDoesNotRenderAsFailure()
    {
        var installer = new RecordingSystemComponentInstaller
        {
            State = ComponentState(
                SystemComponentStatus.AwaitingApproval,
                SystemComponentStepStatus.AwaitingApproval,
                canOpenSystemSettings: true),
            InstallResult = new InstallResult(
                false,
                "请允许 NonProxy 后台项目。",
                "NP_TEST_APPROVAL_REQUIRED"),
        };
        using var services = TestPlatformServices.Create(
            configure: registrations =>
                registrations.AddSingleton<ISystemComponentInstaller>(
                    installer));
        var viewModel = services.GetRequiredService<DashboardViewModel>();

        await viewModel.InstallComponentCommand.ExecuteAsync(null);

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal(
            "请允许 NonProxy 后台项目。",
            viewModel.OperationMessage);
    }

    [Fact]
    public async Task FailedInstallIsVisibleAndDoesNotClaimSuccess()
    {
        var installer = new RecordingSystemComponentInstaller
        {
            State = ComponentState(
                SystemComponentStatus.Failed,
                SystemComponentStepStatus.NeedsRepair),
            InstallResult = new InstallResult(
                false,
                "后台服务仍未就绪。",
                "NP_TEST_NOT_READY"),
        };
        using var services = TestPlatformServices.Create(
            configure: registrations =>
                registrations.AddSingleton<ISystemComponentInstaller>(
                    installer));
        var viewModel = services.GetRequiredService<DashboardViewModel>();

        await viewModel.InstallComponentCommand.ExecuteAsync(null);

        Assert.Equal("修复系统组件", viewModel.State.ComponentActionLabel);
        Assert.Equal("后台服务仍未就绪。", viewModel.ErrorMessage);
        Assert.Equal(1, installer.InstallCalls);
    }

    [Fact]
    public async Task UninstallRequiresExplicitSecondAction()
    {
        var installer = new RecordingSystemComponentInstaller
        {
            State = ComponentState(
                SystemComponentStatus.Installed,
                SystemComponentStepStatus.Ready),
        };
        using var services = TestPlatformServices.Create(
            configure: registrations =>
                registrations.AddSingleton<ISystemComponentInstaller>(
                    installer));
        var viewModel = services.GetRequiredService<DashboardViewModel>();
        await viewModel.RefreshCommand.ExecuteAsync(null);

        viewModel.RequestUninstallCommand.Execute(null);

        Assert.True(viewModel.IsUninstallConfirmationVisible);
        Assert.Equal(0, installer.UninstallCalls);

        await viewModel.ConfirmUninstallCommand.ExecuteAsync(null);

        Assert.False(viewModel.IsUninstallConfirmationVisible);
        Assert.Equal(1, installer.UninstallCalls);
    }

    [Fact]
    public async Task DiagnosticsExposeEveryRuntimeStage()
    {
        var installer = new RecordingSystemComponentInstaller
        {
            State = ComponentState(
                SystemComponentStatus.Failed,
                SystemComponentStepStatus.NeedsRepair),
        };
        using var services = TestPlatformServices.Create(
            configure: registrations =>
                registrations.AddSingleton<ISystemComponentInstaller>(
                    installer));
        var diagnostics = services.GetRequiredService<IDiagnosticsService>();

        var checks = await diagnostics.RunChecksAsync(
            TestContext.Current.CancellationToken);

        Assert.Contains(checks, check => check.Id == "system-component-gateway");
        Assert.Contains(
            checks,
            check => check.Id == "system-component-transparent-proxy");
        Assert.Contains(checks, check => check.Id == "system-component-dns-proxy");
        Assert.Contains(
            checks,
            check => check.Id == "system-component-network-routing");
    }

    private static SystemComponentState ComponentState(
        SystemComponentStatus status,
        SystemComponentStepStatus stepStatus,
        bool canOpenSystemSettings = false)
    {
        return new SystemComponentState(
            status,
            "测试系统组件状态",
            steps:
            [
                new("gateway", "后台服务", stepStatus, "测试后台服务"),
                new(
                    "transparent-proxy",
                    "透明代理",
                    stepStatus,
                    "测试透明代理"),
                new("dns-proxy", "DNS 分流", stepStatus, "测试 DNS 分流"),
                new(
                    "network-routing",
                    "网络接管",
                    stepStatus,
                    "测试网络接管"),
            ],
            canOpenSystemSettings: canOpenSystemSettings);
    }
}

internal sealed class RecordingSystemComponentInstaller
    : ISystemComponentInstaller
{
    public SystemComponentState State { get; set; } = new(
        SystemComponentStatus.NotInstalled,
        "尚未安装");

    public InstallResult InstallResult { get; set; } = new(
        true,
        "安装完成");

    public InstallResult UninstallResult { get; set; } = new(
        true,
        "卸载完成");

    public int InstallCalls { get; private set; }

    public int UninstallCalls { get; private set; }

    public int OpenSettingsCalls { get; private set; }

    public Task<SystemComponentState> GetStateAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(State);
    }

    public Task<InstallResult> InstallAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        InstallCalls++;
        return Task.FromResult(InstallResult);
    }

    public Task<InstallResult> UninstallAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        UninstallCalls++;
        return Task.FromResult(UninstallResult);
    }

    public Task<InstallResult> OpenSystemSettingsAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        OpenSettingsCalls++;
        return Task.FromResult(new InstallResult(true, "系统设置已打开"));
    }
}
