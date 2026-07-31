namespace NonProxy.Desktop.Core.Services.Adapters.Transport;

public sealed record LocalAdapterEndpoint(
    string? SocketPath,
    string? CapabilityPath,
    string? NamedPipePath = null)
{
    public const string StateDirectoryEnvironment = "NONPROXY_ADAPTER_STATE_DIR";
    public const string SocketPathEnvironment = "NONPROXY_ADAPTER_SOCKET_PATH";

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
}
