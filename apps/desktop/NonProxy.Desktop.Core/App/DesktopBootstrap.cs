using Avalonia;
using Avalonia.Logging;

namespace NonProxy.Desktop.Core.Bootstrap;

public static class DesktopBootstrap
{
    public static AppBuilder Build(IServiceProvider services)
    {
        ArgumentNullException.ThrowIfNull(services);

        return AppBuilder
            .Configure<global::NonProxy.Desktop.Core.App>(() => new global::NonProxy.Desktop.Core.App(services))
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace(LogEventLevel.Warning);
    }
}
