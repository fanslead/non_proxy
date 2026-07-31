using NonProxy.Desktop.Core.Services.Adapters.Rpc;
using NonProxy.Desktop.Core.Services.Adapters.Transport;

namespace NonProxy.AdapterDesktopSmoke;

internal static class Program
{
    public static async Task<int> Main(string[] args)
    {
        if (args is not [var stateDirectory]
            || string.IsNullOrWhiteSpace(stateDirectory))
        {
            Console.Error.WriteLine("用法：adapter-desktop-smoke <adapter-state-directory>");
            return 2;
        }

        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        try
        {
            var endpoint = LocalAdapterEndpoint.FromStateDirectory(stateDirectory);
            var channelFactory = new UnixDomainSocketAdapterChannelFactory(endpoint);
            var capabilityProvider = new FileAdapterCapabilityProvider(endpoint);
            var contextProvider = new AdapterRequestContextProvider(capabilityProvider);
            using var client = new GrpcAdapterRpcClient(
                channelFactory,
                contextProvider);
            var response = await client.ListInstallationsAsync(timeout.Token);
            if (response.Error is not null || response.Installations.Count != 0)
            {
                throw new InvalidOperationException(
                    $"隔离适配器目录回读异常：{response.Error?.Code ?? "not-empty"}");
            }

            Console.WriteLine(
                "桌面 Adapter 跨语言联调通过：独立 UDS、独立能力认证和空登记目录回读一致。");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(
                $"桌面 Adapter 跨语言联调失败：{exception.Message}");
            return 1;
        }
    }
}
