using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Desktop.Core.Services.Control.Transport;

namespace NonProxy.ControlSmoke;

internal static class Program
{
    public static async Task<int> Main()
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(20));
        try
        {
            var endpoint = LocalControlEndpoint.FromUnixEnvironment();
            var channelFactory = new UnixDomainSocketControlChannelFactory(endpoint);
            var capabilityProvider = new FileSessionCapabilityProvider(endpoint);
            var contextProvider = new OperationContextProvider(capabilityProvider);
            using var client = new GrpcControlRpcClient(
                channelFactory,
                contextProvider);
            var policies = new GatewayPolicyService(
                client,
                new PolicyContractMapper(new SmokePlatformInformation()));
            var initial = await client.GetSystemStatusAsync(timeout.Token);
            if (initial.ActiveSnapshotVersion != 0
                || initial.PendingSnapshotVersion != 0)
            {
                throw new InvalidOperationException("隔离测试状态目录不是空状态。");
            }

            var saved = await policies.SaveAsync(
                new PolicyDraft(
                    null,
                    "跨语言联调规则",
                    PolicyScope.Website,
                    "smoke.nonproxy.test",
                    PolicyAction.Direct),
                timeout.Token);
            if (!saved.Accepted || saved.Applied || saved.SnapshotVersion != 1)
            {
                throw new InvalidOperationException("规则发布状态不符合待确认语义。");
            }

            var catalog = await policies.GetCatalogAsync(timeout.Token);
            var item = catalog.Items.SingleOrDefault(policy =>
                policy.MatchValue == "smoke.nonproxy.test");
            if (item?.State != PolicyApplyState.Pending
                || catalog.PendingSnapshotVersion != 1)
            {
                throw new InvalidOperationException("跨语言规则状态回读不一致。");
            }

            Console.WriteLine("控制平面跨语言联调通过：UDS、会话认证、写入、发布和状态回读一致。");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine($"控制平面跨语言联调失败：{exception.Message}");
            return 1;
        }
    }

    private sealed class SmokePlatformInformation : IPlatformInformation
    {
        public PlatformKind Platform => PlatformKind.MacOS;

        public string DisplayName => "联调平台";
    }
}
