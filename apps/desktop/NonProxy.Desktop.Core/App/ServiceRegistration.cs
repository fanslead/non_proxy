using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Features.Shell;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Views;

namespace NonProxy.Desktop.Core.Bootstrap;

public static class ServiceRegistration
{
    public static ServiceProvider BuildProvider(Action<IServiceCollection> registerPlatformServices)
    {
        ArgumentNullException.ThrowIfNull(registerPlatformServices);

        var services = new ServiceCollection();
        services.AddSingleton<DashboardViewModel>();
        services.AddSingleton<MainWindowViewModel>();
        services.AddSingleton<MainWindow>();

        registerPlatformServices(services);

        var provider = services.BuildServiceProvider(new ServiceProviderOptions
        {
            ValidateOnBuild = true,
            ValidateScopes = true,
        });

        try
        {
            _ = provider.GetRequiredService<IPlatformInformation>();
            _ = provider.GetRequiredService<ISystemComponentInstaller>();
            return provider;
        }
        catch
        {
            provider.Dispose();
            throw;
        }
    }
}
