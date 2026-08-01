namespace NonProxy.Desktop.Core.Services.Adapters.Transport;

public sealed record LocalAdapterEndpoint(
    string? SocketPath,
    string? CapabilityPath,
    string? NamedPipePath = null)
{
    public const string StateDirectoryEnvironment = "NONPROXY_ADAPTER_STATE_DIR";
    public const string SocketPathEnvironment = "NONPROXY_ADAPTER_SOCKET_PATH";
    public const string WindowsAdapterPipeEnvironment = "NONPROXY_WINDOWS_ADAPTER_PIPE";
    private const string WindowsPipePrefix = @"\\.\pipe\NonProxy.";
    private const int MaximumWindowsSidLength = 184;

    public static LocalAdapterEndpoint Unavailable { get; } = new(null, null);

    public bool IsConfigured =>
        !string.IsNullOrWhiteSpace(CapabilityPath)
        && (string.IsNullOrWhiteSpace(SocketPath)
            != string.IsNullOrWhiteSpace(NamedPipePath));

    public static LocalAdapterEndpoint FromUnixEnvironment(
        string defaultStateDirectory)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(defaultStateDirectory);
        var stateDirectory = Environment.GetEnvironmentVariable(
            StateDirectoryEnvironment);
        if (string.IsNullOrWhiteSpace(stateDirectory))
        {
            stateDirectory = defaultStateDirectory;
        }

        var socketPath = Environment.GetEnvironmentVariable(
            SocketPathEnvironment);
        return FromStateDirectory(stateDirectory, socketPath);
    }

    public static LocalAdapterEndpoint FromStateDirectory(
        string stateDirectory,
        string? socketPath = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(stateDirectory);
        if (!Path.IsPathFullyQualified(stateDirectory))
        {
            throw new ArgumentException(
                "适配器状态目录必须是绝对路径。",
                nameof(stateDirectory));
        }

        socketPath = string.IsNullOrWhiteSpace(socketPath)
            ? Path.Combine(stateDirectory, "adapter-host.sock")
            : socketPath;
        var capabilityPath = Path.Combine(
            stateDirectory,
            "adapter.capability");
        if (!Path.IsPathFullyQualified(socketPath)
            || !string.Equals(
                Path.GetDirectoryName(socketPath),
                stateDirectory,
                StringComparison.Ordinal))
        {
            throw new ArgumentException(
                "适配器套接字必须是状态目录的直接子项。",
                nameof(socketPath));
        }

        return new LocalAdapterEndpoint(socketPath, capabilityPath);
    }

    public static LocalAdapterEndpoint FromWindowsEnvironment(
        string defaultPipePath,
        string? defaultStateDirectory = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(defaultPipePath);
        ValidateWindowsPipe(defaultPipePath);
        var stateDirectory = Environment.GetEnvironmentVariable(
            StateDirectoryEnvironment);
        if (string.IsNullOrWhiteSpace(stateDirectory))
        {
            stateDirectory = defaultStateDirectory;
            if (string.IsNullOrWhiteSpace(stateDirectory))
            {
                var localApplicationData = Environment.GetFolderPath(
                    Environment.SpecialFolder.LocalApplicationData);
                if (string.IsNullOrWhiteSpace(localApplicationData))
                {
                    throw new InvalidOperationException(
                        "无法定位 Windows 用户应用数据目录。");
                }

                stateDirectory = Path.Combine(
                    localApplicationData,
                    "NonProxy",
                    "adapter-host");
            }
        }

        var pipePath = Environment.GetEnvironmentVariable(
            WindowsAdapterPipeEnvironment);
        pipePath = string.IsNullOrWhiteSpace(pipePath)
            ? defaultPipePath
            : pipePath;
        return FromWindowsStateDirectory(stateDirectory, pipePath);
    }

    public static LocalAdapterEndpoint FromWindowsStateDirectory(
        string stateDirectory,
        string pipePath)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(stateDirectory);
        ArgumentException.ThrowIfNullOrWhiteSpace(pipePath);
        if (!Path.IsPathFullyQualified(stateDirectory))
        {
            throw new ArgumentException(
                "适配器状态目录必须是绝对路径。",
                nameof(stateDirectory));
        }

        ValidateWindowsPipe(pipePath);
        return new LocalAdapterEndpoint(
            null,
            Path.Combine(stateDirectory, "adapter.capability"),
            pipePath);
    }

    public static string WindowsPipeForUserSid(string userSid)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(userSid);
        var parts = userSid.Split('-', StringSplitOptions.None);
        var structurallyValid = parts.Length is >= 4 and <= 18
            && string.Equals(parts[0], "S", StringComparison.Ordinal)
            && string.Equals(parts[1], "1", StringComparison.Ordinal)
            && parts.Skip(2).All(part =>
                part.Length > 0
                && part.All(char.IsAsciiDigit)
                && (string.Equals(part, "0", StringComparison.Ordinal)
                    || part[0] != '0'))
            && ulong.TryParse(parts[2], out var identifierAuthority)
            && identifierAuthority <= 0x0000_FFFF_FFFF_FFFF
            && parts.Skip(3).All(part => uint.TryParse(part, out _));
        var serviceIdentity = userSid is "S-1-5-18" or "S-1-5-19" or "S-1-5-20"
            || userSid.StartsWith("S-1-5-80-", StringComparison.Ordinal);
        if (userSid.Length > MaximumWindowsSidLength
            || !structurallyValid
            || serviceIdentity)
        {
            throw new ArgumentException(
                "Windows Adapter 必须绑定普通用户 SID。",
                nameof(userSid));
        }

        var pipePath = $@"{WindowsPipePrefix}Adapter.{userSid}";
        ValidateWindowsPipe(pipePath);
        return pipePath;
    }

    private static void ValidateWindowsPipe(string pipePath)
    {
        var suffix = pipePath.StartsWith(WindowsPipePrefix, StringComparison.Ordinal)
            ? pipePath[WindowsPipePrefix.Length..]
            : string.Empty;
        if (suffix.Length == 0
            || pipePath.Length > 160
            || suffix.Any(character =>
                !char.IsAsciiLetterOrDigit(character)
                && character is not ('.' or '_' or '-')))
        {
            throw new ArgumentException(
                "Windows Adapter 管道必须位于 NonProxy 本地命名空间。",
                nameof(pipePath));
        }
    }
}
