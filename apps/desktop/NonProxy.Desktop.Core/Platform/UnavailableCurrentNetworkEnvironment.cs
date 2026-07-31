namespace NonProxy.Desktop.Core.Platform;

public sealed class UnavailableCurrentNetworkEnvironment :
    ICurrentNetworkEnvironment
{
    public Task<CurrentNetworkEnvironment> CaptureAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(CurrentNetworkEnvironment.Unavailable(
            "当前平台版本尚未提供自动网络识别。"));
    }
}
