using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Services.Adapters;

internal sealed record AdapterApplicationProjection(
    uint Version,
    string Platform,
    string PathKind,
    string Value);

internal static class AdapterApplicationSelectorMapper
{
    public static IEqualityComparer<ApplicationAdapterSelector> Comparer { get; } =
        new SelectorComparer();

    public static bool TryMap(
        ApplicationAdapterSelector selector,
        PlatformKind currentPlatform,
        out AdapterApplicationProjection? projection)
    {
        projection = null;
        if (selector.Version != 1 || selector.Platform != currentPlatform)
        {
            return false;
        }
        var (platform, pathKind, valid) = selector.Kind switch
        {
            ApplicationAdapterSelectorKind.MacOsBundle
                when selector.Platform == PlatformKind.MacOS
                => ("macos", "bundle", IsMacBundle(selector.Value)),
            ApplicationAdapterSelectorKind.WindowsExecutable
                when selector.Platform == PlatformKind.Windows
                => ("windows", "executable", IsWindowsExecutable(selector.Value)),
            _ => (string.Empty, string.Empty, false),
        };
        if (!valid)
        {
            return false;
        }
        projection = new AdapterApplicationProjection(
            selector.Version,
            platform,
            pathKind,
            selector.Value);
        return true;
    }

    private static bool IsMacBundle(string? value)
    {
        return !string.IsNullOrWhiteSpace(value)
            && value.Length <= 4_096
            && value.StartsWith('/')
            && value.EndsWith(".app", StringComparison.Ordinal)
            && !value.Contains('\\')
            && !HasUnsafePathShape(value[1..], '/');
    }

    private static bool IsWindowsExecutable(string? value)
    {
        return !string.IsNullOrWhiteSpace(value)
            && value.Length is >= 7 and <= 4_096
            && char.IsAsciiLetter(value[0])
            && value[1] == ':'
            && value[2] == '\\'
            && value.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)
            && !value.Contains('/')
            && !value[2..].Contains(':')
            && !value.Contains('*')
            && !value.Contains('?')
            && !HasUnsafePathShape(value[3..], '\\');
    }

    private static bool HasUnsafePathShape(string value, char separator)
    {
        return value.Any(character =>
                character == ',' || char.IsControl(character))
            || value.Split(separator).Any(segment =>
                segment is "" or "." or ".."
                || segment.EndsWith(' ')
                || segment.EndsWith('.'));
    }

    private sealed class SelectorComparer :
        IEqualityComparer<ApplicationAdapterSelector>
    {
        public bool Equals(
            ApplicationAdapterSelector? left,
            ApplicationAdapterSelector? right)
        {
            if (ReferenceEquals(left, right))
            {
                return true;
            }
            if (left is null || right is null)
            {
                return false;
            }
            return left.Version == right.Version
                && left.Platform == right.Platform
                && left.Kind == right.Kind
                && string.Equals(
                    left.Value,
                    right.Value,
                    left.Platform == PlatformKind.Windows
                        ? StringComparison.OrdinalIgnoreCase
                        : StringComparison.Ordinal);
        }

        public int GetHashCode(ApplicationAdapterSelector value)
        {
            var comparer = value.Platform == PlatformKind.Windows
                ? StringComparer.OrdinalIgnoreCase
                : StringComparer.Ordinal;
            return HashCode.Combine(
                value.Version,
                value.Platform,
                value.Kind,
                comparer.GetHashCode(value.Value ?? string.Empty));
        }
    }
}
