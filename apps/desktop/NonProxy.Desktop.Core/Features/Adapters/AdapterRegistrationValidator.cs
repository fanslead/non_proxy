namespace NonProxy.Desktop.Core.Features.Adapters;

internal static class AdapterRegistrationValidator
{
    private static readonly StringComparison PathComparison =
        OperatingSystem.IsWindows()
            ? StringComparison.OrdinalIgnoreCase
            : StringComparison.Ordinal;

    public static string? Validate(
        string adapterId,
        string executablePath,
        string mainConfigurationPath,
        string managedRulesPath,
        string directTarget)
    {
        var id = adapterId.Trim();
        if (id.Length is 0 or > 128
            || id.Any(character =>
                !char.IsAsciiLetterOrDigit(character)
                && character is not ('.' or '_' or '-')))
        {
            return "登记名称只能包含字母、数字、点、下划线和连字符。";
        }

        var executable = Normalize(executablePath);
        var main = Normalize(mainConfigurationPath);
        var managed = Normalize(managedRulesPath);
        if (executable is null || main is null || managed is null)
        {
            return "三个文件位置都必须填写绝对路径。";
        }
        if (string.Equals(main, managed, PathComparison))
        {
            return "NonProxy 托管规则必须使用新的独立文件，不能覆盖主配置。";
        }
        if (!string.Equals(
                Path.GetDirectoryName(main),
                Path.GetDirectoryName(managed),
                PathComparison))
        {
            return "托管规则文件必须放在主配置所在目录。";
        }

        var target = directTarget.Trim();
        if (target.Length > 128 || target.Any(char.IsControl))
        {
            return "直连出口名称不能超过 128 个字符，也不能包含控制字符。";
        }

        return null;
    }

    public static string? SuggestManagedRulesPath(
        string mainConfigurationPath,
        string managedFileName)
    {
        var main = Normalize(mainConfigurationPath);
        var directory = main is null ? null : Path.GetDirectoryName(main);
        return string.IsNullOrWhiteSpace(directory)
            ? null
            : Path.Combine(directory, managedFileName);
    }

    private static string? Normalize(string path)
    {
        var trimmed = path.Trim();
        try
        {
            return Path.IsPathFullyQualified(trimmed)
                ? Path.TrimEndingDirectorySeparator(Path.GetFullPath(trimmed))
                : null;
        }
        catch (Exception exception) when (
            exception is ArgumentException
                or NotSupportedException
                or PathTooLongException)
        {
            return null;
        }
    }
}
