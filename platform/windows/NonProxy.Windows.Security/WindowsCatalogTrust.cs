using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Microsoft.Win32.SafeHandles;

namespace NonProxy.Windows.Security;

[SupportedOSPlatform("windows10.0.18362.0")]
public static partial class WindowsCatalogTrust
{
    private const uint MaximumHashBytes = 128;

    public static void VerifyMember(string catalogPath, string memberPath)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(catalogPath);
        ArgumentException.ThrowIfNullOrWhiteSpace(memberPath);
        var resolvedCatalog = Path.GetFullPath(catalogPath);
        var resolvedMember = Path.GetFullPath(memberPath);
        var subsystem = WinTrustNativeConstants.DriverActionVerify;
        if (!CryptCATAdminAcquireContext2(
                out var catalogAdmin,
                ref subsystem,
                "SHA256",
                0,
                0))
        {
            throw new Win32Exception(
                Marshal.GetLastPInvokeError(),
                "无法创建 Windows Driver Catalog 验证上下文。");
        }
        try
        {
            using var member = new FileStream(
                resolvedMember,
                FileMode.Open,
                FileAccess.Read,
                FileShare.Read);
            var hash = CalculateMemberHash(catalogAdmin, member.SafeFileHandle);
            VerifyCatalogMember(
                catalogAdmin,
                resolvedCatalog,
                resolvedMember,
                member.SafeFileHandle,
                hash);
        }
        finally
        {
            _ = CryptCATAdminReleaseContext(catalogAdmin, 0);
        }
    }

    private static byte[] CalculateMemberHash(
        nint catalogAdmin,
        SafeFileHandle file)
    {
        uint size = 0;
        if (!CryptCATAdminCalcHashFromFileHandle2(
                catalogAdmin,
                file.DangerousGetHandle(),
                ref size,
                null,
                0)
            || size == 0
            || size > MaximumHashBytes)
        {
            throw new Win32Exception(
                Marshal.GetLastPInvokeError(),
                "无法确定 Driver Catalog 成员哈希长度。");
        }
        var hash = new byte[size];
        if (!CryptCATAdminCalcHashFromFileHandle2(
                catalogAdmin,
                file.DangerousGetHandle(),
                ref size,
                hash,
                0)
            || size != hash.Length)
        {
            throw new Win32Exception(
                Marshal.GetLastPInvokeError(),
                "无法计算 Driver Catalog 成员哈希。");
        }
        return hash;
    }

    private static void VerifyCatalogMember(
        nint catalogAdmin,
        string catalogPath,
        string memberPath,
        SafeFileHandle memberFile,
        byte[] hash)
    {
        var catalogPointer = Marshal.StringToCoTaskMemUni(catalogPath);
        var memberPointer = Marshal.StringToCoTaskMemUni(memberPath);
        var tagPointer = Marshal.StringToCoTaskMemUni(Convert.ToHexString(hash));
        var hashPointer = Marshal.AllocHGlobal(hash.Length);
        var infoPointer = Marshal.AllocHGlobal(
            Marshal.SizeOf<WinTrustCatalogInfo>());
        try
        {
            Marshal.Copy(hash, 0, hashPointer, hash.Length);
            var info = new WinTrustCatalogInfo
            {
                StructSize = checked(
                    (uint)Marshal.SizeOf<WinTrustCatalogInfo>()),
                CatalogFilePath = catalogPointer,
                MemberTag = tagPointer,
                MemberFilePath = memberPointer,
                MemberFile = memberFile.DangerousGetHandle(),
                CalculatedFileHash = hashPointer,
                CalculatedFileHashSize = checked((uint)hash.Length),
                CatalogAdmin = catalogAdmin,
            };
            Marshal.StructureToPtr(info, infoPointer, false);
            var data = WindowsAuthenticodeTrust.NewTrustData(
                WinTrustNativeConstants.ChoiceCatalog,
                infoPointer);
            var action = WinTrustNativeConstants.DriverActionVerify;
            var status = WindowsAuthenticodeTrust.WinVerifyTrustEx(
                0,
                ref action,
                ref data);
            try
            {
                if (status != 0)
                {
                    throw new InvalidOperationException(
                        $"Driver Catalog 未信任指定成员：0x{status:x8}");
                }
            }
            finally
            {
                data.StateAction = WinTrustNativeConstants.StateActionClose;
                _ = WindowsAuthenticodeTrust.WinVerifyTrustEx(
                    0,
                    ref action,
                    ref data);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(infoPointer);
            Marshal.FreeHGlobal(hashPointer);
            Marshal.FreeCoTaskMem(tagPointer);
            Marshal.FreeCoTaskMem(memberPointer);
            Marshal.FreeCoTaskMem(catalogPointer);
        }
    }

    [LibraryImport(
        "wintrust.dll",
        EntryPoint = "CryptCATAdminAcquireContext2",
        SetLastError = true,
        StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CryptCATAdminAcquireContext2(
        out nint catalogAdmin,
        ref Guid subsystem,
        string? hashAlgorithm,
        nint strongHashPolicy,
        uint flags);

    [LibraryImport(
        "wintrust.dll",
        EntryPoint = "CryptCATAdminCalcHashFromFileHandle2",
        SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CryptCATAdminCalcHashFromFileHandle2(
        nint catalogAdmin,
        nint file,
        ref uint hashSize,
        [Out] byte[]? hash,
        uint flags);

    [LibraryImport(
        "wintrust.dll",
        EntryPoint = "CryptCATAdminReleaseContext",
        SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CryptCATAdminReleaseContext(
        nint catalogAdmin,
        uint flags);
}
