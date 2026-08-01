using System.Runtime.Versioning;
using Avalonia;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Bootstrap;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters.Transport;
using NonProxy.Desktop.Core.Services.Control.Transport;
using NonProxy.Desktop.Windows.ApplicationCatalog;

namespace NonProxy.Desktop.Windows;

internal static class Program
{
    [STAThread]
    [SupportedOSPlatform("windows10.0.18362.0")]
    public static void Main(string[] args)
    {
        _ = WindowsAdapterHostBootstrap.TryStart();
        using var services = ServiceRegistration.BuildProvider(collection =>
        {
            collection.AddSingleton<IPlatformInformation, WindowsPlatformInformation>();
            collection.AddSingleton<
                IWindowsBootstrapPackageLocator,
                WindowsBootstrapPackageLocator>();
            collection.AddSingleton<
                IWindowsBootstrapProcessRunner,
                WindowsBootstrapProcessRunner>();
            collection.AddSingleton<
                IWindowsComponentBootstrap,
                WindowsComponentBootstrap>();
            collection.AddSingleton<ISystemComponentInstaller, WindowsSystemComponentInstaller>();
            collection.AddSingleton<
                IWindowsApplicationDiscovery,
                WindowsApplicationDiscovery>();
            collection.AddSingleton<
                IWindowsPackageDiscovery,
                WindowsPackageDiscovery>();
            collection.AddSingleton<
                IWindowsApplicationIdentityReader,
                WindowsApplicationIdentityReader>();
            collection.AddSingleton<
                IWindowsExecutablePicker,
                AvaloniaWindowsExecutablePicker>();
            collection.AddSingleton<IApplicationCatalog, WindowsApplicationCatalog>();
            collection.AddSingleton(WindowsAdapterEndpointFactory.Create());
            collection.AddSingleton<
                IAdapterChannelFactory,
                WindowsNamedPipeAdapterChannelFactory>();
            collection.AddSingleton<
                IAdapterCapabilityProvider,
                FileAdapterCapabilityProvider>();
            collection.AddSingleton(LocalControlEndpoint.FromWindowsEnvironment());
            collection.AddSingleton<
                IControlChannelFactory,
                WindowsNamedPipeControlChannelFactory>();
            collection.AddSingleton<
                ISessionCapabilityProvider,
                FileSessionCapabilityProvider>();
        });

        DesktopBootstrap
            .Build(services)
            .StartWithClassicDesktopLifetime(args);
    }
}
