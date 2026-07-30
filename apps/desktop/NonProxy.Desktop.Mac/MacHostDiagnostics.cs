using System.Text.Json;

namespace NonProxy.Desktop.Mac;

internal static class MacHostDiagnostics
{
    private const string NativeBridgeSmokeArgument = "--native-bridge-smoke";

    internal static bool IsNativeBridgeSmoke(string[] args)
    {
        return args.Length == 1
            && string.Equals(
                args[0],
                NativeBridgeSmokeArgument,
                StringComparison.Ordinal);
    }

    internal static async Task<int> RunNativeBridgeSmokeAsync()
    {
        try
        {
            var result = await new MacNativeBridgeClient()
                .ProbeAsync(CancellationToken.None);
            Console.WriteLine(JsonSerializer.Serialize(
                result,
                MacNativeJsonContext.Default.MacBridgeProbePayload));
            return result.AbiVersion == MacNativeBridgeClient.SupportedAbiVersion
                && result.Message.Contains("原生桥接", StringComparison.Ordinal)
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
}
