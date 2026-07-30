namespace NonProxy.Desktop.Core.Services.Control.Transport;

public sealed record LocalControlEndpoint(
    string? SocketPath,
    string? SessionCapabilityPath)
{
    public const string StateDirectoryEnvironment = "NONPROXY_STATE_DIR";
    public const string SocketPathEnvironment = "NONPROXY_SOCKET_PATH";

    public static LocalControlEndpoint Unavailable { get; } = new(null, null);

    public bool IsConfigured =>
        !string.IsNullOrWhiteSpace(SocketPath)
        && !string.IsNullOrWhiteSpace(SessionCapabilityPath);

    public static LocalControlEndpoint FromUnixEnvironment()
    {
        var stateDirectory = Environment.GetEnvironmentVariable(StateDirectoryEnvironment);
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

        var socketPath = Environment.GetEnvironmentVariable(SocketPathEnvironment);
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

    private static void ValidateAbsolute(string path, string parameterName)
    {
        if (!Path.IsPathFullyQualified(path))
        {
            throw new ArgumentException("本地控制路径必须是绝对路径。", parameterName);
        }
    }
}
