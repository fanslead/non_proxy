using System.Security.Cryptography;

namespace NonProxy.Desktop.Windows.ApplicationCatalog;

internal static class WindowsApplicationStableIdentity
{
    private const int MaximumCharacters = 2048;

    public static string? Decode(ReadOnlySpan<byte> bytes)
    {
        if (bytes.IsEmpty
            || bytes.Length > MaximumCharacters * sizeof(char)
            || bytes.Length % sizeof(char) != 0)
        {
            return null;
        }

        var characters = new char[bytes.Length / sizeof(char)];
        var count = 0;
        var terminated = false;
        for (var index = 0; index < characters.Length; index++)
        {
            var character = (char)(bytes[index * 2] | bytes[(index * 2) + 1] << 8);
            if (character == '\0')
            {
                terminated = true;
                continue;
            }
            if (terminated)
            {
                return null;
            }
            characters[count++] = character;
        }
        if (count == 0 || count > MaximumCharacters)
        {
            return null;
        }

        var value = new string(characters, 0, count);
        return value.Any(char.IsControl) || HasUnpairedSurrogate(value)
            ? null
            : value;
    }

    public static string? SignerIdentity(ReadOnlySpan<byte> certificate)
    {
        return certificate.IsEmpty
            ? null
            : $"cert-sha256:{Convert.ToHexString(SHA256.HashData(certificate)).ToLowerInvariant()}";
    }

    private static bool HasUnpairedSurrogate(string value)
    {
        for (var index = 0; index < value.Length; index++)
        {
            if (char.IsHighSurrogate(value[index]))
            {
                if (++index >= value.Length || !char.IsLowSurrogate(value[index]))
                {
                    return true;
                }
            }
            else if (char.IsLowSurrogate(value[index]))
            {
                return true;
            }
        }
        return false;
    }
}
