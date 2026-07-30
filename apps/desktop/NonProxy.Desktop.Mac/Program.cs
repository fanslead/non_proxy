using Avalonia;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Bootstrap;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control.Transport;

namespace NonProxy.Desktop.Mac;

internal static class Program
{
    [STAThread]
    public static int Main(string[] args)
    {
        if (MacHostDiagnostics.IsNativeBridgeSmoke(args))
        {
            return MacHostDiagnostics.RunNativeBridgeSmokeAsync()
                .GetAwaiter()
                .GetResult();
        }

        using var services = ServiceRegistration.BuildProvider(collection =>
        {
            collection.AddSingleton<
                IPlatformInformation,
                MacPlatformInformation>();
            collection.AddSingleton<MacNativeBridgeClient>();
            collection.AddSingleton<
                ISystemComponentInstaller,
                SystemExtensionController>();
            collection.AddSingleton(
                LocalControlEndpoint.FromUnixEnvironment(
                    MacRuntimePaths.ResolveDefaultStateDirectory()));
            collection.AddSingleton<
                IControlChannelFactory,
                UnixDomainSocketControlChannelFactory>();
            collection.AddSingleton<
                ISessionCapabilityProvider,
                FileSessionCapabilityProvider>();
        });

        DesktopBootstrap
            .Build(services)
            .StartWithClassicDesktopLifetime(args);
        return 0;
    }
}
