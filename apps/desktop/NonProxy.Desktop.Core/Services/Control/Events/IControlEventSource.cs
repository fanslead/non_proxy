namespace NonProxy.Desktop.Core.Services.Control.Events;

public interface IControlEventSource
{
    IAsyncEnumerable<ControlEventNotification> SubscribeAsync(
        ulong afterSequence,
        CancellationToken cancellationToken);
}
