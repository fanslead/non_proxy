using System.Buffers.Binary;
using System.Globalization;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

internal static class WindowsPackageStableIdentity
{
    private const int PackageSidBytes = 40;
    private const int PackageSubAuthorityCount = 8;
    private const uint PackageAuthority = 15;
    private const uint PackageRid = 2;
    private const int PublisherIdCharacters = 13;

    public static string? DecodeSid(ReadOnlySpan<byte> sid)
    {
        if (sid.Length != PackageSidBytes
            || sid[0] != 1
            || sid[1] != PackageSubAuthorityCount
            || sid[2] != 0
            || sid[3] != 0
            || sid[4] != 0
            || sid[5] != 0
            || sid[6] != 0
            || sid[7] != PackageAuthority
            || ReadSubAuthority(sid, 0) != PackageRid)
        {
            return null;
        }

        var components = new string[PackageSubAuthorityCount];
        for (var index = 0; index < components.Length; index++)
        {
            components[index] = ReadSubAuthority(sid, index)
                .ToString(CultureInfo.InvariantCulture);
        }
        return $"S-1-15-{string.Join('-', components)}";
    }

    public static string? StableIdentity(ReadOnlySpan<byte> sid)
    {
        var canonicalSid = DecodeSid(sid);
        return canonicalSid is null ? null : $"package-sid:{canonicalSid}";
    }

    public static string? SignerIdentity(string? publisherId)
    {
        if (publisherId is null)
        {
            return null;
        }
        if (publisherId.Length != PublisherIdCharacters
            || publisherId.Trim() != publisherId
            || publisherId.Any(character => !IsPublisherIdCharacter(character)))
        {
            return null;
        }
        return $"package-publisher-id:{publisherId.ToLowerInvariant()}";
    }

    private static uint ReadSubAuthority(ReadOnlySpan<byte> sid, int index)
    {
        var offset = 8 + (index * sizeof(uint));
        return BinaryPrimitives.ReadUInt32LittleEndian(
            sid.Slice(offset, sizeof(uint)));
    }

    private static bool IsPublisherIdCharacter(char value)
    {
        return value is >= '0' and <= '9'
            or >= 'A' and <= 'Z'
            or >= 'a' and <= 'z';
    }
}
