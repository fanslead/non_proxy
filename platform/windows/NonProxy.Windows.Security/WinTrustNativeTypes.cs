using System.Runtime.InteropServices;

namespace NonProxy.Windows.Security;

internal static class WinTrustNativeConstants
{
    public const uint UiNone = 2;
    public const uint RevokeNone = 0;
    public const uint ChoiceFile = 1;
    public const uint ChoiceCatalog = 2;
    public const uint StateActionVerify = 1;
    public const uint StateActionClose = 2;
    public const uint CacheOnlyUrlRetrieval = 0x1000;
    public const uint DisableMd2Md4 = 0x2000;

    public static readonly Guid GenericVerifyV2 =
        new("00AAC56B-CD44-11d0-8CC2-00C04FC295EE");

    public static readonly Guid DriverActionVerify =
        new("F750E6C3-38EE-11D1-85E5-00C04FC295EE");
}

[StructLayout(LayoutKind.Sequential)]
internal struct WinTrustFileInfo
{
    public uint StructSize;
    public nint FilePath;
    public nint File;
    public nint KnownSubject;
}

[StructLayout(LayoutKind.Sequential)]
internal struct WinTrustCatalogInfo
{
    public uint StructSize;
    public uint CatalogVersion;
    public nint CatalogFilePath;
    public nint MemberTag;
    public nint MemberFilePath;
    public nint MemberFile;
    public nint CalculatedFileHash;
    public uint CalculatedFileHashSize;
    public nint CatalogContext;
    public nint CatalogAdmin;
}

[StructLayout(LayoutKind.Sequential)]
internal struct WinTrustData
{
    public uint StructSize;
    public nint PolicyCallbackData;
    public nint SipClientData;
    public uint UiChoice;
    public uint RevocationChecks;
    public uint UnionChoice;
    public nint SubjectInfo;
    public uint StateAction;
    public nint StateData;
    public nint UrlReference;
    public uint ProviderFlags;
    public uint UiContext;
    public nint SignatureSettings;
}

[StructLayout(LayoutKind.Sequential)]
internal readonly struct CryptProviderSigner
{
    public readonly uint StructSize;
    public readonly System.Runtime.InteropServices.ComTypes.FILETIME VerifyAsOf;
    public readonly uint CertificateChainCount;
    public readonly nint CertificateChain;
    public readonly uint SignerType;
    public readonly nint Signer;
    public readonly uint Error;
    public readonly uint CounterSignerCount;
    public readonly nint CounterSigners;
    public readonly nint ChainContext;
}

[StructLayout(LayoutKind.Sequential)]
internal readonly struct CryptProviderCertificate
{
    public readonly uint StructSize;
    public readonly nint Certificate;
    public readonly int Commercial;
    public readonly int TrustedRoot;
    public readonly int SelfSigned;
    public readonly int TestCertificate;
    public readonly uint RevokedReason;
    public readonly uint Confidence;
    public readonly uint Error;
    public readonly nint TrustListContext;
    public readonly int TrustListSignerCertificate;
    public readonly nint CtlContext;
    public readonly uint CtlError;
    public readonly int IsCyclic;
    public readonly nint ChainElement;
}

[StructLayout(LayoutKind.Sequential)]
internal readonly struct CertificateContext
{
    public readonly uint EncodingType;
    public readonly nint EncodedCertificate;
    public readonly uint EncodedCertificateSize;
    public readonly nint CertificateInfo;
    public readonly nint CertificateStore;
}
