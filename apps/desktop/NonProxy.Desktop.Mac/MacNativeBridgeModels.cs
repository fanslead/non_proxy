using System.Text.Json.Serialization;

namespace NonProxy.Desktop.Mac;

internal sealed record MacBridgeProbePayload(
    [property: JsonPropertyName("abiVersion")] uint AbiVersion,
    [property: JsonPropertyName("message")] string Message);

internal sealed record MacBridgeDiagnosticError(
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("errorCode")] string ErrorCode,
    [property: JsonPropertyName("message")] string Message);

internal sealed record MacBridgeEventPayload(
    [property: JsonPropertyName("operation")] string Operation,
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("message")] string Message,
    [property: JsonPropertyName("errorCode")] string? ErrorCode,
    [property: JsonPropertyName("requiresReboot")] bool RequiresReboot,
    [property: JsonPropertyName("state")] MacHostState? State);

internal sealed record MacHostState(
    [property: JsonPropertyName("transparentExtension")]
        MacSystemExtensionSnapshot TransparentExtension,
    [property: JsonPropertyName("dnsExtension")]
        MacSystemExtensionSnapshot DnsExtension,
    [property: JsonPropertyName("transparentPreference")]
        MacNetworkPreferenceSnapshot TransparentPreference,
    [property: JsonPropertyName("dnsPreference")]
        MacNetworkPreferenceSnapshot DnsPreference);

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
