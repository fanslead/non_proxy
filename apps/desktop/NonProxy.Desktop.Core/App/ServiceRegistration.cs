using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Activity;
using NonProxy.Desktop.Core.Features.Applications;
using NonProxy.Desktop.Core.Features.Dashboard;
using NonProxy.Desktop.Core.Features.Diagnostics;
using NonProxy.Desktop.Core.Features.Learning;
using NonProxy.Desktop.Core.Features.Outbounds;
using NonProxy.Desktop.Core.Features.Policies;
using NonProxy.Desktop.Core.Features.Settings;
using NonProxy.Desktop.Core.Features.Shell;
using NonProxy.Desktop.Core.Features.Websites;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Desktop.Core.Services.Control.Transport;
using NonProxy.Desktop.Core.Views;

namespace NonProxy.Desktop.Core.Bootstrap;

public static class ServiceRegistration
{
    public static ServiceProvider BuildProvider(Action<IServiceCollection> registerPlatformServices)
    {
        ArgumentNullException.ThrowIfNull(registerPlatformServices);

        var services = new ServiceCollection();
        services.AddSingleton<DashboardViewModel>();
        services.AddSingleton<PoliciesViewModel>();
        services.AddSingleton<ApplicationsViewModel>();
        services.AddSingleton<WebsitesViewModel>();
        services.AddSingleton<OutboundsViewModel>();
        services.AddSingleton<LearningViewModel>();
        services.AddSingleton<ActivityViewModel>();
        services.AddSingleton<DiagnosticsViewModel>();
        services.AddSingleton<SettingsViewModel>();
        services.AddSingleton<MainWindowViewModel>();
        services.AddSingleton<MainWindow>();
        services.AddSingleton<IApplicationCatalog, UnavailableApplicationCatalog>();
        services.AddSingleton<ILocalProxyDiscovery, UnavailableLocalProxyDiscovery>();
        services.AddSingleton<
            ICurrentNetworkEnvironment,
            UnavailableCurrentNetworkEnvironment>();
        services.AddSingleton(LocalControlEndpoint.Unavailable);
        services.AddSingleton<IControlChannelFactory, UnavailableControlChannelFactory>();
        services.AddSingleton<ISessionCapabilityProvider, UnavailableSessionCapabilityProvider>();
        services.AddSingleton<OperationContextProvider>();
        services.AddSingleton<IControlRpcClient, GrpcControlRpcClient>();
        services.AddSingleton<PolicyContractMapper>();
        services.AddSingleton<ISystemStatusService, GatewaySystemStatusService>();
        services.AddSingleton<IPolicyService, GatewayPolicyService>();
        services.AddSingleton<IOutboundService, GatewayOutboundService>();
        services.AddSingleton<ILearningService, DisconnectedLearningService>();
        services.AddSingleton<IActivityService, GatewayActivityService>();
        services.AddSingleton<IDiagnosticsService, GatewayDiagnosticsService>();
        services.AddSingleton<IDesktopSettingsService, DisconnectedDesktopSettingsService>();

        registerPlatformServices(services);
        EnsureRegistered<IPlatformInformation>(services);
        EnsureRegistered<ISystemComponentInstaller>(services);

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

    private static void EnsureRegistered<TService>(IServiceCollection services)
    {
        if (services.Any(descriptor => descriptor.ServiceType == typeof(TService)))
        {
            return;
        }

        throw new InvalidOperationException(
            $"必须注册平台服务 {typeof(TService).Name}。");
    }
}
