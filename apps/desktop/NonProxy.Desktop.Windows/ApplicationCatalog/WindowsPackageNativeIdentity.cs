using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

[SupportedOSPlatform("windows10.0.18362.0")]
internal static partial class WindowsPackageNativeIdentity
{
    private const int MaximumSidBytes = 68;

    public static string? StableIdentity(string packageFamilyName)
    {
        if (string.IsNullOrWhiteSpace(packageFamilyName)
            || packageFamilyName.Any(character => character == '\0' || char.IsControl(character)))
        {
            return null;
        }

        nint sid = 0;
        var result = DeriveAppContainerSidFromAppContainerName(
            packageFamilyName,
            out sid);
        if (result < 0 || sid == 0)
        {
            return null;
        }
        try
        {
            var length = GetLengthSid(sid);
            if (length == 0 || length > MaximumSidBytes)
            {
                return null;
            }
            var bytes = new byte[length];
            Marshal.Copy(sid, bytes, 0, bytes.Length);
            return WindowsPackageStableIdentity.StableIdentity(bytes);
        }
        finally
        {
            _ = FreeSid(sid);
        }
    }

    [LibraryImport(
        "userenv.dll",
        EntryPoint = "DeriveAppContainerSidFromAppContainerName",
        StringMarshalling = StringMarshalling.Utf16)]
    private static partial int DeriveAppContainerSidFromAppContainerName(
        string appContainerName,
        out nint sid);

    [LibraryImport("advapi32.dll")]
    private static partial uint GetLengthSid(nint sid);

    [LibraryImport("advapi32.dll")]
    private static partial nint FreeSid(nint sid);
}
