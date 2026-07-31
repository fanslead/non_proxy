using System.Text.Json.Serialization;

namespace NonProxy.Desktop.Mac;

internal sealed record MacBridgeProbePayload(
    [property: JsonPropertyName("abiVersion")] uint AbiVersion,
    [property: JsonPropertyName("message")] string Message);

internal sealed record MacBridgeDiagnosticError(
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("errorCode")] string ErrorCode,
    [property: JsonPropertyName("message")] string Message);

internal sealed record MacBridgeSmokePayload(
    [property: JsonPropertyName("abiVersion")] uint AbiVersion,
    [property: JsonPropertyName("message")] string Message,
    [property: JsonPropertyName("applicationCatalog")] bool ApplicationCatalog,
    [property: JsonPropertyName("applicationCount")] int ApplicationCount,
    [property: JsonPropertyName("proxyDiscovery")] bool ProxyDiscovery,
    [property: JsonPropertyName("proxyCount")] int ProxyCount);

internal sealed record MacApplicationDescriptor(
    [property: JsonPropertyName("displayName")] string DisplayName,
    [property: JsonPropertyName("stableIdentity")] string StableIdentity,
    [property: JsonPropertyName("signerIdentity")] string? SignerIdentity,
    [property: JsonPropertyName("bundleIdentifier")] string? BundleIdentifier,
    [property: JsonPropertyName("isRunning")] bool IsRunning);

internal sealed record MacApplicationCatalogPayload(
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("message")] string Message,
    [property: JsonPropertyName("errorCode")] string? ErrorCode,
    [property: JsonPropertyName("applications")]
        IReadOnlyList<MacApplicationDescriptor> Applications);

internal sealed record MacApplicationSelectionPayload(
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("message")] string Message,
    [property: JsonPropertyName("errorCode")] string? ErrorCode,
    [property: JsonPropertyName("application")]
        MacApplicationDescriptor? Application);

internal sealed record MacSystemProxyDescriptor(
    [property: JsonPropertyName("suggestedID")] string SuggestedId,
    [property: JsonPropertyName("displayName")] string DisplayName,
    [property: JsonPropertyName("kind")] string Kind,
    [property: JsonPropertyName("host")] string Host,
    [property: JsonPropertyName("port")] ushort Port);

internal sealed record MacSystemProxyDiscoveryPayload(
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("message")] string Message,
    [property: JsonPropertyName("errorCode")] string? ErrorCode,
    [property: JsonPropertyName("proxies")]
        IReadOnlyList<MacSystemProxyDescriptor> Proxies);

internal sealed record MacNetworkFingerprintDescriptor(
    [property: JsonPropertyName("kind")] string Kind,
    [property: JsonPropertyName("value")] string Value);

internal sealed record MacCurrentNetworkPayload(
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("message")] string Message,
    [property: JsonPropertyName("errorCode")] string? ErrorCode,
    [property: JsonPropertyName("permissionState")] string PermissionState,
    [property: JsonPropertyName("suggestedName")] string? SuggestedName,
    [property: JsonPropertyName("fingerprint")]
        MacNetworkFingerprintDescriptor? Fingerprint);

internal sealed record MacBridgeEventPayload(
    [property: JsonPropertyName("operation")] string Operation,
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("message")] string Message,
    [property: JsonPropertyName("errorCode")] string? ErrorCode,
    [property: JsonPropertyName("requiresReboot")] bool RequiresReboot,
    [property: JsonPropertyName("state")] MacHostState? State);

internal sealed record MacHostState(
    [property: JsonPropertyName("gatewayAgent")]
        MacBackgroundAgentSnapshot GatewayAgent,
    [property: JsonPropertyName("adapterHostAgent")]
        MacBackgroundAgentSnapshot AdapterHostAgent,
    [property: JsonPropertyName("transparentExtension")]
        MacSystemExtensionSnapshot TransparentExtension,
    [property: JsonPropertyName("dnsExtension")]
        MacSystemExtensionSnapshot DnsExtension,
    [property: JsonPropertyName("transparentPreference")]
        MacNetworkPreferenceSnapshot TransparentPreference,
    [property: JsonPropertyName("dnsPreference")]
        MacNetworkPreferenceSnapshot DnsPreference);

internal sealed record MacBackgroundAgentSnapshot(
    [property: JsonPropertyName("registered")] bool Registered,
    [property: JsonPropertyName("enabled")] bool Enabled,
    [property: JsonPropertyName("requiresApproval")] bool RequiresApproval,
    [property: JsonPropertyName("found")] bool Found,
    [property: JsonPropertyName("ready")] bool Ready,
    [property: JsonPropertyName("requiresUpgrade")] bool RequiresUpgrade);

internal sealed record MacSystemExtensionSnapshot(
    [property: JsonPropertyName("bundleIdentifier")] string BundleIdentifier,
    [property: JsonPropertyName("installed")] bool Installed,
    [property: JsonPropertyName("enabled")] bool Enabled,
    [property: JsonPropertyName("awaitingUserApproval")]
        bool AwaitingUserApproval,
    [property: JsonPropertyName("uninstalling")] bool Uninstalling,
    [property: JsonPropertyName("bundleVersion")] string? BundleVersion,
    [property: JsonPropertyName("bundleShortVersion")]
        string? BundleShortVersion);

internal sealed record MacNetworkPreferenceSnapshot(
    [property: JsonPropertyName("configured")] bool Configured,
    [property: JsonPropertyName("enabled")] bool Enabled);

internal sealed class MacNativeBridgeException(
    string errorCode,
    string message,
    Exception? innerException = null) : Exception(message, innerException)
{
    internal string ErrorCode { get; } = errorCode;
}
