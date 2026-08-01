using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

[SupportedOSPlatform("windows")]
internal sealed class WindowsApplicationIdentityReader : IWindowsApplicationIdentityReader
{
    public ApplicationCatalogEntry? Read(WindowsApplicationCandidate candidate)
    {
        var path = WindowsApplicationDiscovery.NormalizeExecutablePath(
            candidate.ExecutablePath);
        if (path is null)
        {
            return null;
        }
        var stableIdentity = WindowsApplicationNativeIdentity.StableIdentity(path);
        var signerIdentity = WindowsAuthenticodeVerifier.TrustedSignerIdentity(path);
        if (stableIdentity is null || signerIdentity is null)
        {
            return null;
        }

        return new ApplicationCatalogEntry(
            WindowsApplicationDiscovery.DisplayNameFor(path),
            stableIdentity,
            signerIdentity,
            null,
            candidate.IsRunning,
            null,
            false);
    }
}

[SupportedOSPlatform("windows")]
internal static partial class WindowsApplicationNativeIdentity
{
    private const int MaximumBlobBytes = 4096;

    public static string? StableIdentity(string path)
    {
        nint blobPointer = 0;
        var code = FwpmGetAppIdFromFileName0(path, out blobPointer);
        if (code != 0 || blobPointer == 0)
        {
            return null;
        }
        try
        {
            var blob = Marshal.PtrToStructure<FwpByteBlob>(blobPointer);
            if (blob.Size == 0
                || blob.Size > MaximumBlobBytes
                || blob.Data == 0)
            {
                return null;
            }
            var bytes = new byte[blob.Size];
            Marshal.Copy(blob.Data, bytes, 0, bytes.Length);
            return WindowsApplicationStableIdentity.Decode(bytes);
        }
        finally
        {
            FwpmFreeMemory0(ref blobPointer);
        }
    }

    [LibraryImport(
        "fwpuclnt.dll",
        EntryPoint = "FwpmGetAppIdFromFileName0",
        StringMarshalling = StringMarshalling.Utf16)]
    private static partial uint FwpmGetAppIdFromFileName0(
        string fileName,
        out nint applicationId);

    [LibraryImport("fwpuclnt.dll", EntryPoint = "FwpmFreeMemory0")]
    private static partial void FwpmFreeMemory0(ref nint memory);

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct FwpByteBlob
    {
        public readonly uint Size;
        public readonly nint Data;
    }
}

[SupportedOSPlatform("windows")]
internal static partial class WindowsAuthenticodeVerifier
{
    private const uint WtdUiNone = 2;
    private const uint WtdRevokeNone = 0;
    private const uint WtdChoiceFile = 1;
    private const uint WtdStateActionVerify = 1;
    private const uint WtdStateActionClose = 2;
    private const uint WtdCacheOnlyUrlRetrieval = 0x1000;
    private const uint WtdDisableMd2Md4 = 0x2000;
    private const int MaximumCertificateBytes = 1024 * 1024;
    private static readonly Guid GenericVerifyV2 =
        new("00AAC56B-CD44-11d0-8CC2-00C04FC295EE");

    public static string? TrustedSignerIdentity(string path)
    {
        var pathPointer = Marshal.StringToCoTaskMemUni(path);
        var filePointer = Marshal.AllocHGlobal(
            Marshal.SizeOf<WinTrustFileInfo>());
        try
        {
            var file = new WinTrustFileInfo
            {
                StructSize = checked((uint)Marshal.SizeOf<WinTrustFileInfo>()),
                FilePath = pathPointer,
            };
            Marshal.StructureToPtr(file, filePointer, false);
            var data = new WinTrustData
            {
                StructSize = checked((uint)Marshal.SizeOf<WinTrustData>()),
                UiChoice = WtdUiNone,
                RevocationChecks = WtdRevokeNone,
                UnionChoice = WtdChoiceFile,
                FileInfo = filePointer,
                StateAction = WtdStateActionVerify,
                ProviderFlags = WtdCacheOnlyUrlRetrieval | WtdDisableMd2Md4,
            };
            var action = GenericVerifyV2;
            var status = WinVerifyTrustEx(0, ref action, ref data);
            try
            {
                return status == 0
                    ? SignerCertificateHash(data.StateData)
                    : null;
            }
            finally
            {
                data.StateAction = WtdStateActionClose;
                _ = WinVerifyTrustEx(0, ref action, ref data);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(filePointer);
            Marshal.FreeCoTaskMem(pathPointer);
        }
    }

    private static string? SignerCertificateHash(nint stateData)
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
        var signerPointer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
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
        var certificate = Marshal.PtrToStructure<CertificateContext>(
            providerCertificate.Certificate);
        if (certificate.EncodedCertificate == 0
            || certificate.EncodedCertificateSize == 0
            || certificate.EncodedCertificateSize > MaximumCertificateBytes)
        {
            return null;
        }
        var bytes = new byte[certificate.EncodedCertificateSize];
        Marshal.Copy(certificate.EncodedCertificate, bytes, 0, bytes.Length);
        return WindowsApplicationStableIdentity.SignerIdentity(bytes);
    }

    [LibraryImport("wintrust.dll", EntryPoint = "WinVerifyTrustEx")]
    private static partial int WinVerifyTrustEx(
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

    [StructLayout(LayoutKind.Sequential)]
    private struct WinTrustFileInfo
    {
        public uint StructSize;
        public nint FilePath;
        public nint File;
        public nint KnownSubject;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct WinTrustData
    {
        public uint StructSize;
        public nint PolicyCallbackData;
        public nint SipClientData;
        public uint UiChoice;
        public uint RevocationChecks;
        public uint UnionChoice;
        public nint FileInfo;
        public uint StateAction;
        public nint StateData;
        public nint UrlReference;
        public uint ProviderFlags;
        public uint UiContext;
        public nint SignatureSettings;
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct CryptProviderSigner
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
    private readonly struct CryptProviderCertificate
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
    private readonly struct CertificateContext
    {
        public readonly uint EncodingType;
        public readonly nint EncodedCertificate;
        public readonly uint EncodedCertificateSize;
        public readonly nint CertificateInfo;
        public readonly nint CertificateStore;
    }
}
