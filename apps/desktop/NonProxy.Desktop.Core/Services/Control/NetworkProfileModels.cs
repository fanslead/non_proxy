using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Services.Control;

public sealed record NetworkProfileListItem(
    string Id,
    string DisplayName,
    NetworkFingerprintKind FingerprintKind,
    string FingerprintValue,
    ulong Revision)
{
    public string FingerprintKindLabel => FingerprintKind switch
    {
        NetworkFingerprintKind.WiFiSsidSha256 => "Wi-Fi 精确指纹",
        NetworkFingerprintKind.DefaultGatewaySha256 => "局域网网关指纹",
        NetworkFingerprintKind.InterfaceClass => "网络类型",
        _ => "未知指纹",
    };

    public string FingerprintPreview => FingerprintKind == NetworkFingerprintKind.InterfaceClass
        ? FingerprintValue switch
        {
            "wifi" => "Wi-Fi",
            "ethernet" => "有线网络",
            "cellular" => "蜂窝网络",
            _ => "其他网络",
        }
        : FingerprintValue.Length > 16
            ? $"{FingerprintValue[..8]}…{FingerprintValue[^6..]}"
            : FingerprintValue;

    public string AccuracyLabel => FingerprintKind switch
    {
        NetworkFingerprintKind.WiFiSsidSha256 => "精确匹配当前 Wi-Fi",
        NetworkFingerprintKind.DefaultGatewaySha256 => "按物理网关匹配；路由器变化后需重新检测",
        NetworkFingerprintKind.InterfaceClass => "同类型网络可能共用此配置",
        _ => "无法判断匹配精度",
    };
}

public sealed record NetworkProfileCatalog(
    IReadOnlyList<NetworkProfileListItem> Items,
    ulong CatalogGeneration,
    DateTimeOffset CapturedAt)
{
    public static NetworkProfileCatalog Empty { get; } = new(
        Array.Empty<NetworkProfileListItem>(),
        0,
        DateTimeOffset.MinValue);
}

public sealed record NetworkProfileDraft(
    string? ExistingId,
    string DisplayName,
    NetworkFingerprintKind FingerprintKind,
    string FingerprintValue,
    ulong? ExistingRevision = null);

public sealed record NetworkProfileMutation(
    bool Accepted,
    string Code,
    string Message,
    NetworkProfileListItem? Profile);
