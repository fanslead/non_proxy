using Avalonia;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Bootstrap;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Windows;

internal static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        using var services = ServiceRegistration.BuildProvider(collection =>
        {
            collection.AddSingleton<IPlatformInformation, WindowsPlatformInformation>();
            collection.AddSingleton<ISystemComponentInstaller, WindowsSystemComponentInstaller>();
        });

        DesktopBootstrap
            .Build(services)
            .StartWithClassicDesktopLifetime(args);
    }
}
