namespace NonProxy.Desktop.Core.Services.Control.Transport;

public sealed record LocalControlEndpoint(
    string? SocketPath,
    string? SessionCapabilityPath,
    string? NamedPipePath = null)
{
    public const string StateDirectoryEnvironment = "NONPROXY_STATE_DIR";
    public const string SocketPathEnvironment = "NONPROXY_SOCKET_PATH";
    public const string DefaultWindowsControlPipe = @"\\.\pipe\NonProxy.Control.v1";
    private const string WindowsPipePrefix = @"\\.\pipe\NonProxy.";

    public static LocalControlEndpoint Unavailable { get; } = new(null, null, null);

    public bool IsConfigured =>
        !string.IsNullOrWhiteSpace(SessionCapabilityPath)
        && (string.IsNullOrWhiteSpace(SocketPath)
            != string.IsNullOrWhiteSpace(NamedPipePath));

    public static LocalControlEndpoint FromUnixEnvironment(
        string? defaultStateDirectory = null)
    {
        var stateDirectory = Environment.GetEnvironmentVariable(StateDirectoryEnvironment);
        if (string.IsNullOrWhiteSpace(stateDirectory))
        {
            stateDirectory = defaultStateDirectory;
            if (string.IsNullOrWhiteSpace(stateDirectory))
            {
                var applicationData = Environment.GetFolderPath(
                    Environment.SpecialFolder.ApplicationData);
                if (string.IsNullOrWhiteSpace(applicationData))
                {
                    throw new InvalidOperationException("无法定位 Unix 应用数据目录。");
                }

                stateDirectory = Path.Combine(applicationData, "NonProxy");
            }
        }

        var socketPath = Environment.GetEnvironmentVariable(SocketPathEnvironment);
        return FromStateDirectory(stateDirectory, socketPath);
    }

    public static LocalControlEndpoint FromStateDirectory(
        string stateDirectory,
        string? socketPath = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(stateDirectory);
        if (string.IsNullOrWhiteSpace(socketPath))
        {
            socketPath = Path.Combine(stateDirectory, "gatewayd.sock");
        }

        var capabilityPath = Path.Combine(stateDirectory, "session.capability");
        ValidateAbsolute(stateDirectory, nameof(stateDirectory));
        ValidateAbsolute(socketPath, nameof(socketPath));
        ValidateAbsolute(capabilityPath, nameof(capabilityPath));
        if (!string.Equals(
                Path.GetDirectoryName(socketPath),
                stateDirectory,
                StringComparison.Ordinal))
        {
            throw new InvalidOperationException("控制套接字必须位于状态目录中。");
        }

        return new LocalControlEndpoint(socketPath, capabilityPath);
    }

    public static LocalControlEndpoint FromWindowsStateDirectory(
        string stateDirectory,
        string? pipePath = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(stateDirectory);
        pipePath = string.IsNullOrWhiteSpace(pipePath)
            ? DefaultWindowsControlPipe
            : pipePath;
        ValidateAbsolute(stateDirectory, nameof(stateDirectory));
        ValidateWindowsPipe(pipePath);
        return new LocalControlEndpoint(
            null,
            Path.Combine(stateDirectory, "session.capability"),
            pipePath);
    }

    private static void ValidateAbsolute(string path, string parameterName)
    {
        if (!Path.IsPathFullyQualified(path))
        {
            throw new ArgumentException("本地控制路径必须是绝对路径。", parameterName);
        }
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
                "Windows 控制管道必须位于 NonProxy 本地命名空间。",
                nameof(pipePath));
        }
    }
}
