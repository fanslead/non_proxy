namespace NonProxy.Desktop.Core.Services.Control.Events;

public interface IControlEventMonitor
{
    event Action<ConnectionState>? StateChanged;

    event Action<ControlEventNotification>? EventReceived;

    ConnectionState State { get; }

    Task RunAsync(CancellationToken cancellationToken);
}
