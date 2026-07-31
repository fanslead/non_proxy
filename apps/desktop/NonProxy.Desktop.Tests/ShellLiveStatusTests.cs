using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Shell;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Events;

namespace NonProxy.Desktop.Tests;

public sealed class ShellLiveStatusTests
{
    [AvaloniaFact]
    public async Task InterruptedStreamIsVisibleGloballyAndOnDashboard()
    {
        var monitor = new ControlledEventMonitor();
        using var services = TestPlatformServices.Create(
            configure: registrations =>
                registrations.AddSingleton<IControlEventMonitor>(monitor));
        var shell = services.GetRequiredService<MainWindowViewModel>();

        await shell.InitializeCommand.ExecuteAsync(null);
        await monitor.Started.Task.WaitAsync(TestContext.Current.CancellationToken);
        monitor.EmitState(ConnectionState.Interrupted);
        Dispatcher.UIThread.RunJobs();

        Assert.Equal("状态同步：已中断，正在重连", shell.LiveStatusLabel);
        Assert.Equal("状态更新中断", shell.Dashboard.State.ConnectionLabel);
    }

    [AvaloniaFact]
    public async Task DisposedShellIgnoresQueuedMonitorCallbacks()
    {
        var monitor = new ControlledEventMonitor();
        using var services = TestPlatformServices.Create(
            configure: registrations =>
                registrations.AddSingleton<IControlEventMonitor>(monitor));
        var shell = services.GetRequiredService<MainWindowViewModel>();

        await shell.InitializeCommand.ExecuteAsync(null);
        await monitor.Started.Task.WaitAsync(TestContext.Current.CancellationToken);
        monitor.EmitState(ConnectionState.Interrupted);
        shell.Dispose();
        Dispatcher.UIThread.RunJobs();

        Assert.Equal("状态同步：尚未启动", shell.LiveStatusLabel);
    }

    [AvaloniaFact]
    public async Task SnapshotEventRereadsAuthoritativeDashboardState()
    {
        var monitor = new ControlledEventMonitor();
        var status = new CountingSystemStatusService();
        using var services = TestPlatformServices.Create(
            configure: registrations =>
            {
                registrations.AddSingleton<IControlEventMonitor>(monitor);
                registrations.AddSingleton<ISystemStatusService>(status);
            });
        var shell = services.GetRequiredService<MainWindowViewModel>();

        await shell.InitializeCommand.ExecuteAsync(null);
        await monitor.Started.Task.WaitAsync(TestContext.Current.CancellationToken);
        monitor.EmitEvent(new ControlEventNotification(1, ControlEventKind.Snapshot));
        await PumpUiUntilAsync(status.SecondRead.Task);

        Assert.Equal(2, status.ReadCount);
    }

    [AvaloniaFact]
    public async Task DecisionEventDoesNotReloadSetupCatalogs()
    {
        var monitor = new ControlledEventMonitor();
        var status = new CountingSystemStatusService();
        var outbounds = new DashboardRefreshIsolationTests.CountingOutboundService();
        var adapters = new DashboardRefreshIsolationTests.CountingAdapterService();
        using var services = TestPlatformServices.Create(
            configure: registrations =>
            {
                registrations.AddSingleton<IControlEventMonitor>(monitor);
                registrations.AddSingleton<ISystemStatusService>(status);
                registrations.AddSingleton<IOutboundService>(outbounds);
                registrations.AddSingleton<IAdapterManagementService>(adapters);
            });
        var shell = services.GetRequiredService<MainWindowViewModel>();

        await shell.InitializeCommand.ExecuteAsync(null);
        await monitor.Started.Task.WaitAsync(TestContext.Current.CancellationToken);
        monitor.EmitEvent(new ControlEventNotification(1, ControlEventKind.Decision));
        await PumpUiUntilAsync(status.SecondRead.Task);

        Assert.Equal(2, status.ReadCount);
        Assert.Equal(1, outbounds.ReadCount);
        Assert.Equal(1, adapters.ReadCount);
    }

    private static async Task PumpUiUntilAsync(Task completion)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(
            TestContext.Current.CancellationToken);
        timeout.CancelAfter(TimeSpan.FromSeconds(5));
        while (!completion.IsCompleted)
        {
            await Task.Delay(TimeSpan.FromMilliseconds(20), timeout.Token);
            Dispatcher.UIThread.RunJobs();
        }

        await completion;
    }

    private sealed class ControlledEventMonitor : IControlEventMonitor
    {
        public TaskCompletionSource Started { get; } = new(
            TaskCreationOptions.RunContinuationsAsynchronously);

        public event Action<ConnectionState>? StateChanged;

        public event Action<ControlEventNotification>? EventReceived;

        public ConnectionState State { get; private set; } = ConnectionState.Disconnected;

        public async Task RunAsync(CancellationToken cancellationToken)
        {
            Started.TrySetResult();
            await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
        }

        public void EmitState(ConnectionState state)
        {
            State = state;
            StateChanged?.Invoke(state);
        }

        public void EmitEvent(ControlEventNotification notification)
        {
            EventReceived?.Invoke(notification);
        }
    }

    private sealed class CountingSystemStatusService : ISystemStatusService
    {
        public TaskCompletionSource SecondRead { get; } = new(
            TaskCreationOptions.RunContinuationsAsynchronously);

        public int ReadCount { get; private set; }

        public Task<SystemOverview> GetOverviewAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ReadCount++;
            if (ReadCount == 2)
            {
                SecondRead.TrySetResult();
            }

            return Task.FromResult(new SystemOverview(
                ConnectionState.Connected,
                new SystemComponentState(
                    SystemComponentStatus.NotInstalled,
                    "测试组件未安装"),
                "测试状态",
                "测试详情",
                null,
                0,
                0,
                0,
                0,
                DateTimeOffset.UtcNow));
        }
    }
}
