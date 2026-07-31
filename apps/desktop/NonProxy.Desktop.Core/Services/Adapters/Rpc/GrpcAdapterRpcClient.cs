using Google.Protobuf;
using Grpc.Core;
using Grpc.Net.Client;
using NonProxy.Adapter.V1;
using NonProxy.Desktop.Core.Services.Adapters.Transport;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Services.Adapters.Rpc;

public sealed class GrpcAdapterRpcClient : IAdapterRpcClient, IDisposable
{
    private static readonly TimeSpan ReadTimeout = TimeSpan.FromSeconds(8);
    private static readonly TimeSpan MutationTimeout = TimeSpan.FromSeconds(30);
    private readonly Lazy<GrpcChannel> _channel;
    private readonly Lazy<AdapterService.AdapterServiceClient> _client;
    private readonly AdapterRequestContextProvider _contextProvider;

    public GrpcAdapterRpcClient(
        IAdapterChannelFactory channelFactory,
        AdapterRequestContextProvider contextProvider)
    {
        ArgumentNullException.ThrowIfNull(channelFactory);
        _contextProvider = contextProvider;
        _channel = new Lazy<GrpcChannel>(
            channelFactory.CreateChannel,
            LazyThreadSafetyMode.ExecutionAndPublication);
        _client = new Lazy<AdapterService.AdapterServiceClient>(
            () => new AdapterService.AdapterServiceClient(_channel.Value),
            LazyThreadSafetyMode.ExecutionAndPublication);
    }

    private AdapterService.AdapterServiceClient Client => _client.Value;

    public async Task<ListInstallationsResponse> ListInstallationsAsync(
        CancellationToken cancellationToken)
    {
        var context = await _contextProvider.CreateAsync(
            "list-adapters",
            cancellationToken);
        return await ExecuteAsync(() => Client.ListInstallationsAsync(
            new ListInstallationsRequest { Context = context },
            Options(ReadTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<RegisterInstallationResponse> RegisterInstallationAsync(
        string adapterId,
        AdapterClient client,
        string executablePath,
        string managedRulesPath,
        string mainConfigurationPath,
        string? directTarget,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(adapterId);
        ValidateClient(client);
        ArgumentException.ThrowIfNullOrWhiteSpace(executablePath);
        ArgumentException.ThrowIfNullOrWhiteSpace(managedRulesPath);
        ArgumentException.ThrowIfNullOrWhiteSpace(mainConfigurationPath);
        var context = await _contextProvider.CreateAsync(
            "register-adapter",
            cancellationToken);
        return await ExecuteAsync(() => Client.RegisterInstallationAsync(
            new RegisterInstallationRequest
            {
                Context = context,
                AdapterId = adapterId,
                Client = client,
                ExecutablePath = executablePath,
                ManagedRulesPath = managedRulesPath,
                MainConfigurationPath = mainConfigurationPath,
                DirectTarget = directTarget?.Trim() ?? string.Empty,
            },
            Options(MutationTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<RemoveInstallationResponse> RemoveInstallationAsync(
        string adapterId,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(adapterId);
        var context = await _contextProvider.CreateAsync(
            "remove-adapter",
            cancellationToken);
        return await ExecuteAsync(() => Client.RemoveInstallationAsync(
            new RemoveInstallationRequest
            {
                Context = context,
                AdapterId = adapterId,
            },
            Options(MutationTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<DetectResponse> DetectAsync(
        string adapterId,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(adapterId);
        var context = await _contextProvider.CreateAsync(
            "detect-adapter",
            cancellationToken);
        return await ExecuteAsync(() => Client.DetectAsync(
            new DetectRequest
            {
                Context = context,
                AdapterId = adapterId,
            },
            Options(ReadTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<ReadCapabilitiesResponse> ReadCapabilitiesAsync(
        string adapterId,
        string installationId,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(adapterId);
        ValidateIdentifier(installationId);
        var context = await _contextProvider.CreateAsync(
            "read-adapter-capabilities",
            cancellationToken);
        return await ExecuteAsync(() => Client.ReadCapabilitiesAsync(
            new ReadCapabilitiesRequest
            {
                Context = context,
                AdapterId = adapterId,
                InstallationId = installationId,
            },
            Options(ReadTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<PrepareChangeResponse> PrepareChangeAsync(
        string adapterId,
        string installationId,
        byte[] normalizedPolicy,
        byte[] normalizedPolicyHash,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(adapterId);
        ValidateIdentifier(installationId);
        ArgumentNullException.ThrowIfNull(normalizedPolicy);
        ValidateHash(normalizedPolicyHash, nameof(normalizedPolicyHash));
        var context = await _contextProvider.CreateAsync(
            "prepare-adapter-change",
            cancellationToken);
        return await ExecuteAsync(() => Client.PrepareChangeAsync(
            new PrepareChangeRequest
            {
                Context = context,
                AdapterId = adapterId,
                InstallationId = installationId,
                NormalizedPolicy = ByteString.CopyFrom(normalizedPolicy),
                NormalizedPolicyHash = ByteString.CopyFrom(normalizedPolicyHash),
            },
            Options(MutationTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<ApplyChangeResponse> ApplyChangeAsync(
        string changeId,
        byte[] candidateHash,
        byte[] configurationCandidateHash,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(changeId);
        ValidateHash(candidateHash, nameof(candidateHash));
        ValidateHash(
            configurationCandidateHash,
            nameof(configurationCandidateHash));
        var context = await _contextProvider.CreateAsync(
            "apply-adapter-change",
            cancellationToken);
        return await ExecuteAsync(() => Client.ApplyChangeAsync(
            new ApplyChangeRequest
            {
                Context = context,
                ChangeId = changeId,
                ExpectedCandidateHash = ByteString.CopyFrom(candidateHash),
                ExpectedConfigurationCandidateHash = ByteString.CopyFrom(
                    configurationCandidateHash),
            },
            Options(MutationTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<VerifyChangeResponse> VerifyChangeAsync(
        string changeId,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(changeId);
        var context = await _contextProvider.CreateAsync(
            "verify-adapter-change",
            cancellationToken);
        return await ExecuteAsync(() => Client.VerifyChangeAsync(
            new VerifyChangeRequest
            {
                Context = context,
                ChangeId = changeId,
            },
            Options(ReadTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<RollbackChangeResponse> RollbackChangeAsync(
        string changeId,
        string backupId,
        CancellationToken cancellationToken)
    {
        ValidateIdentifier(changeId);
        ValidateIdentifier(backupId);
        var context = await _contextProvider.CreateAsync(
            "rollback-adapter-change",
            cancellationToken);
        return await ExecuteAsync(() => Client.RollbackChangeAsync(
            new RollbackChangeRequest
            {
                Context = context,
                ChangeId = changeId,
                BackupId = backupId,
            },
            Options(MutationTimeout, cancellationToken)).ResponseAsync);
    }

    public void Dispose()
    {
        if (_channel.IsValueCreated)
        {
            _channel.Value.Dispose();
        }
    }

    private static CallOptions Options(
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        return new CallOptions(
            deadline: DateTime.UtcNow.Add(timeout),
            cancellationToken: cancellationToken);
    }

    private static async Task<TResponse> ExecuteAsync<TResponse>(
        Func<Task<TResponse>> operation)
    {
        try
        {
            return await operation();
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (ControlServiceException)
        {
            throw;
        }
        catch (RpcException exception)
        {
            throw AdapterRpcExceptionMapper.FromRpc(exception);
        }
        catch (HttpRequestException exception)
        {
            throw new ControlServiceException(
                "NP_ADAPTER_UNAVAILABLE",
                "适配器后台未启动或正在重启，请稍后重试。",
                exception);
        }
        catch (IOException exception)
        {
            throw new ControlServiceException(
                "NP_ADAPTER_UNAVAILABLE",
                "无法连接本地适配器套接字，请确认系统组件正在运行。",
                exception);
        }
    }

    private static void ValidateIdentifier(string value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(value);
        if (value.Length > 128
            || value.Any(character =>
                !char.IsAsciiLetterOrDigit(character)
                && character is not ('.' or '_' or '-')))
        {
            throw new ArgumentException("适配器标识格式无效。", nameof(value));
        }
    }

    private static void ValidateClient(AdapterClient client)
    {
        if (client is not (AdapterClient.Surge
            or AdapterClient.Mihomo
            or AdapterClient.SingBox))
        {
            throw new ArgumentOutOfRangeException(nameof(client));
        }
    }

    private static void ValidateHash(byte[] hash, string parameterName)
    {
        ArgumentNullException.ThrowIfNull(hash, parameterName);
        if (hash.Length != 32)
        {
            throw new ArgumentException(
                "适配器哈希必须是 32 字节 SHA-256。",
                parameterName);
        }
    }
}
