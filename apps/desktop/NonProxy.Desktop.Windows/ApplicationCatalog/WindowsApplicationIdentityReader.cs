using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Windows.Security;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

[SupportedOSPlatform("windows10.0.18362.0")]
internal sealed class WindowsApplicationIdentityReader : IWindowsApplicationIdentityReader
{
    public ApplicationCatalogEntry? Read(WindowsApplicationCandidate candidate)
    {
        if (candidate.Kind == WindowsApplicationCandidateKind.Package)
        {
            return ReadPackage(candidate);
        }

        var path = WindowsApplicationDiscovery.NormalizeExecutablePath(
            candidate.IdentitySource);
        if (path is null)
        {
            return null;
        }
        var stableIdentity = WindowsApplicationNativeIdentity.StableIdentity(path);
        var signerIdentity = WindowsAuthenticodeTrust.VerifyFile(path)?.StableIdentity;
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

    private static ApplicationCatalogEntry? ReadPackage(
        WindowsApplicationCandidate candidate)
    {
        var stableIdentity = WindowsPackageNativeIdentity.StableIdentity(
            candidate.IdentitySource);
        var signerIdentity = WindowsPackageStableIdentity.SignerIdentity(
            candidate.PackagePublisherId);
        if (stableIdentity is null || signerIdentity is null)
        {
            return null;
        }

        return new ApplicationCatalogEntry(
            WindowsApplicationDiscovery.CleanPackageDisplayName(
                candidate.DisplayName,
                candidate.IdentitySource),
            stableIdentity,
            signerIdentity,
            candidate.IdentitySource,
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
