namespace NonProxy.Desktop.Core.Services.Adapters.Transport;

public sealed record LocalAdapterEndpoint(
    string? SocketPath,
    string? CapabilityPath,
    string? NamedPipePath = null)
{
    public const string StateDirectoryEnvironment = "NONPROXY_ADAPTER_STATE_DIR";
    public const string SocketPathEnvironment = "NONPROXY_ADAPTER_SOCKET_PATH";
    public const string WindowsAdapterPipeEnvironment = "NONPROXY_WINDOWS_ADAPTER_PIPE";
    public const string DefaultWindowsAdapterPipe = @"\\.\pipe\NonProxy.Adapter.v1";
    private const string WindowsPipePrefix = @"\\.\pipe\NonProxy.";

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
        string? defaultStateDirectory = null)
    {
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
        return FromWindowsStateDirectory(stateDirectory, pipePath);
    }

    public static LocalAdapterEndpoint FromWindowsStateDirectory(
        string stateDirectory,
        string? pipePath = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(stateDirectory);
        if (!Path.IsPathFullyQualified(stateDirectory))
        {
            throw new ArgumentException(
                "适配器状态目录必须是绝对路径。",
                nameof(stateDirectory));
        }

        pipePath = string.IsNullOrWhiteSpace(pipePath)
            ? DefaultWindowsAdapterPipe
            : pipePath;
        ValidateWindowsPipe(pipePath);
        return new LocalAdapterEndpoint(
            null,
            Path.Combine(stateDirectory, "adapter.capability"),
            pipePath);
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
