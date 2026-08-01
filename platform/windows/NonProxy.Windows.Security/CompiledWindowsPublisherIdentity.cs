using System.Reflection;

namespace NonProxy.Windows.Security;

public static class CompiledWindowsPublisherIdentity
{
    private const string MetadataName =
        "NonProxyWindowsPublisherCertificateSha256";

    public static string? Read(Assembly assembly)
    {
        ArgumentNullException.ThrowIfNull(assembly);
        var value = assembly
            .GetCustomAttributes<AssemblyMetadataAttribute>()
            .SingleOrDefault(attribute => attribute.Key == MetadataName)
            ?.Value
            ?.Trim()
            .ToLowerInvariant();
        return IsCanonicalCertificateSha256(value) ? value : null;
    }

    public static bool IsCanonicalCertificateSha256(string? value) =>
        value is { Length: 64 }
        && value.All(character =>
            char.IsAsciiDigit(character) || character is >= 'a' and <= 'f');
}
