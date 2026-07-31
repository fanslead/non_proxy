using System.IO;

namespace NonProxy.Desktop.Core.Services.Control.Events;

public sealed class ControlEventMonitor : IControlEventMonitor
{
    private static readonly TimeSpan DefaultInitialDelay = TimeSpan.FromMilliseconds(250);
    private static readonly TimeSpan DefaultMaximumDelay = TimeSpan.FromSeconds(5);

    private readonly IControlEventSource _source;
    private readonly TimeSpan _initialDelay;
    private readonly TimeSpan _maximumDelay;
    private int _running;

    public ControlEventMonitor(IControlEventSource source)
        : this(source, DefaultInitialDelay, DefaultMaximumDelay)
    {
    }

    internal ControlEventMonitor(
        IControlEventSource source,
        TimeSpan initialDelay,
        TimeSpan maximumDelay)
    {
        ArgumentNullException.ThrowIfNull(source);
        if (initialDelay <= TimeSpan.Zero || maximumDelay < initialDelay)
        {
            throw new ArgumentOutOfRangeException(
                nameof(initialDelay),
                "重连延迟必须为正数，且最大值不得小于初始值。");
        }

        _source = source;
        _initialDelay = initialDelay;
        _maximumDelay = maximumDelay;
    }

    public event Action<ConnectionState>? StateChanged;

    public event Action<ControlEventNotification>? EventReceived;

    public ConnectionState State { get; private set; } = ConnectionState.Disconnected;

    public async Task RunAsync(CancellationToken cancellationToken)
    {
        if (Interlocked.Exchange(ref _running, 1) != 0)
        {
            throw new InvalidOperationException("控制事件监视器已经在运行。");
        }

        try
        {
            var retryDelay = _initialDelay;
            while (!cancellationToken.IsCancellationRequested)
            {
                SetState(ConnectionState.Connecting);
                try
                {
                    var previousSequence = 0UL;
                    await foreach (var notification in _source
                        .SubscribeAsync(0, cancellationToken)
                        .WithCancellation(cancellationToken))
                    {
                        if (notification.Kind == ControlEventKind.StreamReady)
                        {
                            SetState(ConnectionState.Connected);
                            retryDelay = _initialDelay;
                            continue;
                        }

                        ValidateSequence(notification.Sequence, previousSequence);
                        previousSequence = notification.Sequence;
                        SetState(ConnectionState.Connected);
                        EventReceived?.Invoke(notification);
                    }

                    SetState(ConnectionState.Interrupted);
                }
                catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
                {
                    break;
                }
                catch (Exception)
                {
                    SetState(ConnectionState.Interrupted);
                }

                try
                {
                    await Task.Delay(retryDelay, cancellationToken);
                }
                catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
                {
                    break;
                }

                retryDelay = TimeSpan.FromMilliseconds(
                    Math.Min(
                        retryDelay.TotalMilliseconds * 2,
                        _maximumDelay.TotalMilliseconds));
            }
        }
        finally
        {
            Interlocked.Exchange(ref _running, 0);
            SetState(ConnectionState.Disconnected);
        }
    }

    private static void ValidateSequence(ulong sequence, ulong previousSequence)
    {
        if (sequence == 0 || sequence <= previousSequence)
        {
            throw new InvalidDataException("控制事件序号无效或未严格递增。");
        }
    }

    private void SetState(ConnectionState value)
    {
        if (State == value)
        {
            return;
        }

        State = value;
        StateChanged?.Invoke(value);
    }
}
