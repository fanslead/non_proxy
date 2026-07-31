using System.Runtime.CompilerServices;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Events;

namespace NonProxy.Desktop.Tests;

public sealed class ControlEventMonitorTests
{
    [Fact]
    public async Task HandshakeEventAndInterruptionProduceTruthfulStates()
    {
        var source = new ScriptedEventSource(
            ControlEventNotification.Ready,
            new ControlEventNotification(1, ControlEventKind.Snapshot));
        var monitor = new ControlEventMonitor(
            source,
            TimeSpan.FromMilliseconds(1),
            TimeSpan.FromMilliseconds(2));
        var states = new List<ConnectionState>();
        var events = new List<ControlEventNotification>();
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        monitor.EventReceived += events.Add;
        monitor.StateChanged += state =>
        {
            states.Add(state);
            if (state == ConnectionState.Interrupted)
            {
                cancellation.Cancel();
            }
        };

        await monitor.RunAsync(cancellation.Token);

        Assert.Equal(
            [
                ConnectionState.Connecting,
                ConnectionState.Connected,
                ConnectionState.Interrupted,
                ConnectionState.Disconnected,
            ],
            states);
        Assert.Equal(
            new ControlEventNotification(1, ControlEventKind.Snapshot),
            Assert.Single(events));
        Assert.Equal([0UL], source.AfterSequences);
    }

    [Fact]
    public async Task NonIncreasingSequenceInterruptsBeforeDeliveringDuplicate()
    {
        var source = new ScriptedEventSource(
            ControlEventNotification.Ready,
            new ControlEventNotification(3, ControlEventKind.Snapshot),
            new ControlEventNotification(3, ControlEventKind.Decision));
        var monitor = new ControlEventMonitor(
            source,
            TimeSpan.FromMilliseconds(1),
            TimeSpan.FromMilliseconds(2));
        var events = new List<ControlEventNotification>();
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        monitor.EventReceived += events.Add;
        monitor.StateChanged += state =>
        {
            if (state == ConnectionState.Interrupted)
            {
                cancellation.Cancel();
            }
        };

        await monitor.RunAsync(cancellation.Token);

        Assert.Equal(
            new ControlEventNotification(3, ControlEventKind.Snapshot),
            Assert.Single(events));
    }

    [Fact]
    public async Task UnexpectedSourceFailureReconnectsInsteadOfStoppingMonitor()
    {
        var source = new FailOnceEventSource();
        var monitor = new ControlEventMonitor(
            source,
            TimeSpan.FromMilliseconds(1),
            TimeSpan.FromMilliseconds(2));
        var states = new List<ConnectionState>();
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        monitor.StateChanged += states.Add;
        monitor.EventReceived += _ => cancellation.Cancel();

        await monitor.RunAsync(cancellation.Token);

        Assert.Equal(2, source.Attempts);
        Assert.Contains(ConnectionState.Interrupted, states);
        Assert.Contains(ConnectionState.Connected, states);
        Assert.Equal(ConnectionState.Disconnected, states[^1]);
    }

    private sealed class ScriptedEventSource(
        params ControlEventNotification[] notifications)
        : IControlEventSource
    {
        public List<ulong> AfterSequences { get; } = [];

        public async IAsyncEnumerable<ControlEventNotification> SubscribeAsync(
            ulong afterSequence,
            [EnumeratorCancellation] CancellationToken cancellationToken)
        {
            AfterSequences.Add(afterSequence);
            foreach (var notification in notifications)
            {
                cancellationToken.ThrowIfCancellationRequested();
                yield return notification;
            }

            await Task.Yield();
            throw new ControlServiceException(
                "NP_CONTROL_INTERRUPTED",
                "测试流已中断。");
        }
    }

    private sealed class FailOnceEventSource : IControlEventSource
    {
        public int Attempts { get; private set; }

        public async IAsyncEnumerable<ControlEventNotification> SubscribeAsync(
            ulong afterSequence,
            [EnumeratorCancellation] CancellationToken cancellationToken)
        {
            _ = afterSequence;
            Attempts++;
            if (Attempts == 1)
            {
                await Task.Yield();
                throw new InvalidOperationException("模拟意外的传输层异常。");
            }

            yield return ControlEventNotification.Ready;
            yield return new ControlEventNotification(1, ControlEventKind.Snapshot);
            await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
        }
    }
}
