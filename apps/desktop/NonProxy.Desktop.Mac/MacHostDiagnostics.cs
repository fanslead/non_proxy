using System.Text.Json;
using AppKit;
using Foundation;

namespace NonProxy.Desktop.Mac;

internal static class MacHostDiagnostics
{
    private const string NativeBridgeSmokeArgument = "--native-bridge-smoke";
    private const string QueryArgument = "--system-components-query";
    private const string InstallArgument = "--system-components-install";
    private const string UninstallArgument = "--system-components-uninstall";
    private const string MutationConsentVariable =
        "NONPROXY_ALLOW_SYSTEM_MUTATION";
    private static readonly TimeSpan QueryTimeout =
        TimeSpan.FromSeconds(15);

    internal static bool IsNativeBridgeSmoke(string[] args)
    {
        return args.Length == 1
            && string.Equals(
                args[0],
                NativeBridgeSmokeArgument,
                StringComparison.Ordinal);
    }

    internal static int RunWithMainRunLoop(
        Func<Task<int>> operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        NSApplication.Init();
        var task = operation();
        while (!task.IsCompleted)
        {
            NSRunLoop.Current.RunUntil(
                NSDate.FromTimeIntervalSinceNow(0.05));
        }
        return task.GetAwaiter().GetResult();
    }

    internal static async Task<int> RunNativeBridgeSmokeAsync()
    {
        try
        {
            var result = await new MacNativeBridgeClient()
                .ProbeAsync(CancellationToken.None);
            var applications = await new MacNativeBridgeClient()
                .ListApplicationsAsync(CancellationToken.None);
            var proxies = await new MacNativeBridgeClient()
                .DiscoverSystemProxiesAsync(CancellationToken.None);
            var smoke = new MacBridgeSmokePayload(
                result.AbiVersion,
                result.Message,
                applications.Success && applications.Applications.Count > 0,
                applications.Applications.Count,
                proxies.Success,
                proxies.Proxies.Count);
            Console.WriteLine(JsonSerializer.Serialize(
                smoke,
                MacNativeJsonContext.Default.MacBridgeSmokePayload));
            return result.AbiVersion == MacNativeBridgeClient.SupportedAbiVersion
                && result.Message.Contains("原生桥接", StringComparison.Ordinal)
                && applications.Success
                && applications.Applications.Count > 0
                && proxies.Success
                    ? 0
                    : 1;
        }
        catch (MacNativeBridgeException exception)
        {
            var error = new MacBridgeDiagnosticError(
                false,
                exception.ErrorCode,
                exception.Message);
            Console.Error.WriteLine(JsonSerializer.Serialize(
                error,
                MacNativeJsonContext.Default.MacBridgeDiagnosticError));
            return 1;
        }
    }

    internal static bool TryGetSystemComponentAction(
        string[] args,
        out MacSystemComponentAction action)
    {
        action = default;
        if (args.Length != 1)
        {
            return false;
        }
        action = args[0] switch
        {
            QueryArgument => MacSystemComponentAction.Query,
            InstallArgument => MacSystemComponentAction.Install,
            UninstallArgument => MacSystemComponentAction.Uninstall,
            _ => default,
        };
        return args[0] is QueryArgument
            or InstallArgument
            or UninstallArgument;
    }

    internal static async Task<int> RunSystemComponentActionAsync(
        MacSystemComponentAction action)
    {
        if (action != MacSystemComponentAction.Query
            && !HasSystemMutationConsent())
        {
            WriteDiagnosticError(
                "NP_MAC_SYSTEM_MUTATION_NOT_CONFIRMED",
                "安装或卸载系统组件前必须显式确认网络状态变更。");
            return 64;
        }

        try
        {
            var client = new MacNativeBridgeClient();
            using var queryCancellation = action
                == MacSystemComponentAction.Query
                    ? new CancellationTokenSource(QueryTimeout)
                    : new CancellationTokenSource();
            var result = action switch
            {
                MacSystemComponentAction.Query =>
                    await client.QueryAsync(queryCancellation.Token),
                MacSystemComponentAction.Install =>
                    await client.InstallAndEnableAsync(
                        WriteApprovalNotice,
                        operationCompleted: static () => { },
                        CancellationToken.None),
                MacSystemComponentAction.Uninstall =>
                    await client.DisableAndUninstallAsync(
                        CancellationToken.None),
                _ => throw new InvalidOperationException(
                    "无法识别系统组件诊断动作。"),
            };
            Console.WriteLine(JsonSerializer.Serialize(
                result,
                MacNativeJsonContext.Default.MacBridgeEventPayload));
            return result.Success ? 0 : 1;
        }
        catch (OperationCanceledException)
        {
            WriteDiagnosticError(
                "NP_MAC_SYSTEM_QUERY_TIMEOUT",
                "macOS 未在限定时间内返回系统组件状态。");
            return 1;
        }
        catch (MacNativeBridgeException exception)
        {
            WriteDiagnosticError(
                exception.ErrorCode,
                exception.Message);
            return 1;
        }
    }

    internal static bool HasSystemMutationConsent()
    {
        return string.Equals(
            Environment.GetEnvironmentVariable(MutationConsentVariable),
            "1",
            StringComparison.Ordinal);
    }

    private static void WriteApprovalNotice()
    {
        Console.Error.WriteLine(
            "macOS 正在等待用户允许 NonProxy 后台项目或网络扩展。");
    }

    private static void WriteDiagnosticError(
        string errorCode,
        string message)
    {
        var error = new MacBridgeDiagnosticError(
            false,
            errorCode,
            message);
        Console.Error.WriteLine(JsonSerializer.Serialize(
            error,
            MacNativeJsonContext.Default.MacBridgeDiagnosticError));
    }
}

internal enum MacSystemComponentAction
{
    Query,
    Install,
    Uninstall,
}
