using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;

namespace NonProxy.Windows.Security;

[SupportedOSPlatform("windows")]
public static partial class WindowsAuthenticodeTrust
{
    private const int MaximumCertificateBytes = 1024 * 1024;

    public static WindowsSignerCertificate? VerifyFile(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        var resolved = Path.GetFullPath(path);
        var pathPointer = Marshal.StringToCoTaskMemUni(resolved);
        var filePointer = Marshal.AllocHGlobal(Marshal.SizeOf<WinTrustFileInfo>());
        try
        {
            var file = new WinTrustFileInfo
            {
                StructSize = checked((uint)Marshal.SizeOf<WinTrustFileInfo>()),
                FilePath = pathPointer,
            };
            Marshal.StructureToPtr(file, filePointer, false);
            var data = NewTrustData(
                WinTrustNativeConstants.ChoiceFile,
                filePointer);
            var action = WinTrustNativeConstants.GenericVerifyV2;
            var status = WinVerifyTrustEx(0, ref action, ref data);
            try
            {
                return status == 0
                    ? ReadSignerCertificate(data.StateData)
                    : null;
            }
            finally
            {
                data.StateAction = WinTrustNativeConstants.StateActionClose;
                _ = WinVerifyTrustEx(0, ref action, ref data);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(filePointer);
            Marshal.FreeCoTaskMem(pathPointer);
        }
    }

    internal static WinTrustData NewTrustData(
        uint subjectChoice,
        nint subjectInfo)
    {
        return new WinTrustData
        {
            StructSize = checked((uint)Marshal.SizeOf<WinTrustData>()),
            UiChoice = WinTrustNativeConstants.UiNone,
            RevocationChecks = WinTrustNativeConstants.RevokeNone,
            UnionChoice = subjectChoice,
            SubjectInfo = subjectInfo,
            StateAction = WinTrustNativeConstants.StateActionVerify,
            ProviderFlags = WinTrustNativeConstants.CacheOnlyUrlRetrieval
                | WinTrustNativeConstants.DisableMd2Md4,
        };
    }

    private static WindowsSignerCertificate? ReadSignerCertificate(
        nint stateData)
    {
        if (stateData == 0)
        {
            return null;
        }
        var provider = WTHelperProvDataFromStateData(stateData);
        if (provider == 0)
        {
            return null;
        }
        var signerPointer = WTHelperGetProvSignerFromChain(
            provider,
            0,
            false,
            0);
        if (signerPointer == 0)
        {
            return null;
        }
        var signer = Marshal.PtrToStructure<CryptProviderSigner>(signerPointer);
        if (signer.CertificateChainCount == 0 || signer.CertificateChain == 0)
        {
            return null;
        }
        var providerCertificate = Marshal.PtrToStructure<CryptProviderCertificate>(
            signer.CertificateChain);
        if (providerCertificate.Certificate == 0)
        {
            return null;
        }
        var context = Marshal.PtrToStructure<CertificateContext>(
            providerCertificate.Certificate);
        if (context.EncodedCertificate == 0
            || context.EncodedCertificateSize == 0
            || context.EncodedCertificateSize > MaximumCertificateBytes)
        {
            return null;
        }
        var encoded = new byte[context.EncodedCertificateSize];
        Marshal.Copy(context.EncodedCertificate, encoded, 0, encoded.Length);
        using var certificate = X509CertificateLoader.LoadCertificate(encoded);
        return new WindowsSignerCertificate(
            certificate.Thumbprint.ToUpperInvariant(),
            Convert.ToHexString(SHA256.HashData(encoded)).ToLowerInvariant());
    }

    [LibraryImport("wintrust.dll", EntryPoint = "WinVerifyTrustEx")]
    internal static partial int WinVerifyTrustEx(
        nint window,
        ref Guid action,
        ref WinTrustData data);

    [LibraryImport("wintrust.dll", EntryPoint = "WTHelperProvDataFromStateData")]
    private static partial nint WTHelperProvDataFromStateData(nint stateData);

    [LibraryImport("wintrust.dll", EntryPoint = "WTHelperGetProvSignerFromChain")]
    private static partial nint WTHelperGetProvSignerFromChain(
        nint providerData,
        uint signerIndex,
        [MarshalAs(UnmanagedType.Bool)] bool counterSigner,
        uint counterSignerIndex);
}
