using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Mac;

internal sealed class MacCurrentNetworkEnvironment(
    MacNativeBridgeClient bridge) : ICurrentNetworkEnvironment
{
    public async Task<CurrentNetworkEnvironment> CaptureAsync(
        CancellationToken cancellationToken)
    {
        var payload = await bridge.CaptureCurrentNetworkAsync(cancellationToken);
        if (!payload.Success || payload.Fingerprint is null)
        {
            return CurrentNetworkEnvironment.Unavailable(payload.Message);
        }

        return new CurrentNetworkEnvironment(
            true,
            payload.SuggestedName ?? "当前网络",
            ParseKind(payload.Fingerprint.Kind),
            payload.Fingerprint.Value,
            payload.PermissionState,
            payload.Message);
    }

    private static NetworkFingerprintKind ParseKind(string value)
    {
        return value switch
        {
            "wifi_ssid_sha256" => NetworkFingerprintKind.WiFiSsidSha256,
            "default_gateway_sha256" => NetworkFingerprintKind.DefaultGatewaySha256,
            "interface_class" => NetworkFingerprintKind.InterfaceClass,
            _ => throw new MacNativeBridgeException(
                "NP_MAC_NETWORK_FINGERPRINT_INVALID",
                "原生桥接返回了无法识别的网络指纹类型。"),
        };
    }
}
