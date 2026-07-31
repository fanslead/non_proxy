using System.Text;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Policy.V1;
using PlatformFingerprintKind = NonProxy.Desktop.Core.Platform.NetworkFingerprintKind;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public static class NetworkProfileContractMapper
{
    public static (NetworkProfileSpec Profile, ulong ExpectedRevision) ToContract(
        NetworkProfileDraft draft)
    {
        ArgumentNullException.ThrowIfNull(draft);
        var displayName = ValidateDisplayName(draft.DisplayName);

        ValidateFingerprint(draft.FingerprintKind, draft.FingerprintValue);
        var expectedRevision = draft.ExistingRevision ?? 0;
        ulong revision;
        string id;
        if (draft.ExistingId is null)
        {
            if (draft.ExistingRevision is not null)
            {
                throw InvalidContract("新网络配置不应携带已有修订号。");
            }

            id = $"user-network-{Guid.NewGuid():N}";
            revision = 1;
        }
        else
        {
            ValidateIdentifier(draft.ExistingId);
            if (expectedRevision == 0 || expectedRevision == ulong.MaxValue)
            {
                throw InvalidContract("编辑网络配置时缺少有效修订号，请刷新后重试。");
            }

            id = draft.ExistingId;
            revision = expectedRevision + 1;
        }

        return (new NetworkProfileSpec
        {
            Id = id,
            DisplayName = displayName,
            FingerprintKind = ToContractKind(draft.FingerprintKind),
            FingerprintValue = draft.FingerprintValue,
            Revision = revision,
        }, expectedRevision);
    }

    public static NetworkProfileListItem FromContract(NetworkProfileSpec profile)
    {
        ArgumentNullException.ThrowIfNull(profile);
        var kind = FromContractKind(profile.FingerprintKind);
        ValidateFingerprint(kind, profile.FingerprintValue);
        ValidateIdentifier(profile.Id);
        var displayName = ValidateDisplayName(profile.DisplayName);
        if (profile.Revision == 0)
        {
            throw InvalidContract("控制服务返回了无效网络配置档。");
        }

        return new NetworkProfileListItem(
            profile.Id,
            displayName,
            kind,
            profile.FingerprintValue,
            profile.Revision);
    }

    private static PlatformFingerprintKind FromContractKind(
        NonProxy.Policy.V1.NetworkFingerprintKind kind)
    {
        return kind switch
        {
            NonProxy.Policy.V1.NetworkFingerprintKind.WifiSsidSha256 =>
                PlatformFingerprintKind.WiFiSsidSha256,
            NonProxy.Policy.V1.NetworkFingerprintKind.DefaultGatewaySha256 =>
                PlatformFingerprintKind.DefaultGatewaySha256,
            NonProxy.Policy.V1.NetworkFingerprintKind.InterfaceClass =>
                PlatformFingerprintKind.InterfaceClass,
            _ => throw InvalidContract("控制服务返回了未知网络指纹类型。"),
        };
    }

    private static NonProxy.Policy.V1.NetworkFingerprintKind ToContractKind(
        PlatformFingerprintKind kind)
    {
        return kind switch
        {
            PlatformFingerprintKind.WiFiSsidSha256 =>
                NonProxy.Policy.V1.NetworkFingerprintKind.WifiSsidSha256,
            PlatformFingerprintKind.DefaultGatewaySha256 =>
                NonProxy.Policy.V1.NetworkFingerprintKind.DefaultGatewaySha256,
            PlatformFingerprintKind.InterfaceClass =>
                NonProxy.Policy.V1.NetworkFingerprintKind.InterfaceClass,
            _ => throw InvalidContract("未知网络指纹类型。"),
        };
    }

    private static void ValidateFingerprint(
        PlatformFingerprintKind kind,
        string value)
    {
        var valid = kind switch
        {
            PlatformFingerprintKind.WiFiSsidSha256
                or PlatformFingerprintKind.DefaultGatewaySha256 =>
                value.Length == 64 && value.All(character =>
                    char.IsAsciiDigit(character) || character is >= 'a' and <= 'f'),
            PlatformFingerprintKind.InterfaceClass =>
                value is "wifi" or "ethernet" or "cellular" or "other",
            _ => false,
        };
        if (!valid)
        {
            throw InvalidContract("网络指纹格式无效，请重新检测当前网络。");
        }
    }

    private static string ValidateDisplayName(string value)
    {
        var displayName = value.Trim();
        if (string.IsNullOrEmpty(displayName)
            || Encoding.UTF8.GetByteCount(displayName) > 128
            || displayName.Any(char.IsControl))
        {
            throw InvalidContract("网络名称不能为空，且最多为 128 个 UTF-8 字节。");
        }

        return displayName;
    }

    private static void ValidateIdentifier(string value)
    {
        var valid = !string.IsNullOrEmpty(value)
            && value.Length <= 128
            && string.Equals(value, value.Trim(), StringComparison.Ordinal)
            && value.All(character => char.IsAsciiLetterOrDigit(character)
                || character is '.' or '_' or ':' or '-');
        if (!valid)
        {
            throw InvalidContract("网络配置标识无效。");
        }
    }

    private static ControlServiceException InvalidContract(string message)
    {
        return new ControlServiceException("NP_CONTROL_CONTRACT_INVALID", message);
    }
}
