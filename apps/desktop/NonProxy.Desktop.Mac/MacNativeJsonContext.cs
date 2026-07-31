using System.Text.Json.Serialization;

namespace NonProxy.Desktop.Mac;

[JsonSourceGenerationOptions(
    PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase)]
[JsonSerializable(typeof(MacBridgeProbePayload))]
[JsonSerializable(typeof(MacBridgeDiagnosticError))]
[JsonSerializable(typeof(MacBridgeSmokePayload))]
[JsonSerializable(typeof(MacApplicationCatalogPayload))]
[JsonSerializable(typeof(MacApplicationSelectionPayload))]
[JsonSerializable(typeof(MacBridgeEventPayload))]
internal sealed partial class MacNativeJsonContext : JsonSerializerContext;
