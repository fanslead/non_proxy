using System.Security.Cryptography;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using Grpc.Core;
using Grpc.Net.Client;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Events;
using NonProxy.Desktop.Core.Services.Control.Transport;
using ProtoPolicy = NonProxy.Policy.V1.Policy;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient :
    IControlRpcClient,
    IControlEventSource,
    IDisposable
{
    private static readonly TimeSpan ReadTimeout = TimeSpan.FromSeconds(5);
    private static readonly TimeSpan MutationTimeout = TimeSpan.FromSeconds(15);
    private static readonly TimeSpan OutboundProbeTimeout = TimeSpan.FromSeconds(5);
    private static readonly TimeSpan OutboundProbeRpcTimeout = TimeSpan.FromSeconds(10);

    private readonly Lazy<GrpcChannel> _channel;
    private readonly Lazy<ControlService.ControlServiceClient> _client;
    private readonly OperationContextProvider _contextProvider;

    public GrpcControlRpcClient(
        IControlChannelFactory channelFactory,
        OperationContextProvider contextProvider)
    {
        ArgumentNullException.ThrowIfNull(channelFactory);
        _contextProvider = contextProvider;
        _channel = new Lazy<GrpcChannel>(
            channelFactory.CreateChannel,
            LazyThreadSafetyMode.ExecutionAndPublication);
        _client = new Lazy<ControlService.ControlServiceClient>(
            () => new ControlService.ControlServiceClient(_channel.Value),
            LazyThreadSafetyMode.ExecutionAndPublication);
    }

    private ControlService.ControlServiceClient Client => _client.Value;

    public async Task<UpsertPolicyResponse> UpsertPolicyAsync(
        ProtoPolicy policy,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(policy);
        var context = await _contextProvider.CreateAsync(
            "upsert-policy",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.UpsertPolicyAsync(
                new UpsertPolicyRequest
                {
                    Context = context,
                    Policy = policy,
                    ExpectedRevision = expectedRevision,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<DeletePolicyResponse> DeletePolicyAsync(
        string policyId,
        ulong expectedRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(policyId);
        var context = await _contextProvider.CreateAsync(
            "delete-policy",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.DeletePolicyAsync(
                new DeletePolicyRequest
                {
                    Context = context,
                    PolicyId = policyId,
                    ExpectedRevision = expectedRevision,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<ApplyPolicySnapshotResponse> ApplySnapshotAsync(
        CancellationToken cancellationToken)
    {
        var context = await _contextProvider.CreateAsync(
            "apply-snapshot",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.ApplyPolicySnapshotAsync(
                new ApplyPolicySnapshotRequest { Context = context },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<RollbackPolicySnapshotResponse> RollBackAsync(
        ulong snapshotVersion,
        CancellationToken cancellationToken)
    {
        ArgumentOutOfRangeException.ThrowIfZero(snapshotVersion);

        var context = await _contextProvider.CreateAsync(
            "rollback-snapshot",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.RollbackPolicySnapshotAsync(
                new RollbackPolicySnapshotRequest
                {
                    Context = context,
                    TargetSnapshotVersion = snapshotVersion,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<ImportConfigurationResponse> ImportConfigurationAsync(
        string format,
        byte[] configuration,
        bool validateOnly,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(format);
        ArgumentNullException.ThrowIfNull(configuration);
        if (configuration.Length == 0)
        {
            throw new ArgumentException("代理配置不能为空。", nameof(configuration));
        }

        try
        {
            var context = await _contextProvider.CreateAsync(
                "import-outbound",
                cancellationToken);
            var request = new ImportConfigurationRequest
            {
                Context = context,
                Format = format,
                Configuration = UnsafeByteOperations.UnsafeWrap(configuration),
                ValidateOnly = validateOnly,
            };
            return await ExecuteAsync(
                () => Client.ImportConfigurationAsync(
                    request,
                    MutationOptions(cancellationToken)).ResponseAsync);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(configuration);
        }
    }

    public async Task<TestOutboundResponse> TestOutboundAsync(
        string outboundId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        var context = await _contextProvider.CreateAsync(
            "test-outbound",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.TestOutboundAsync(
                CreateTestOutboundRequest(outboundId, context),
                Options(OutboundProbeRpcTimeout, cancellationToken)).ResponseAsync);
    }

    public async Task<SetDefaultRouteResponse> SetDefaultRouteAsync(
        string outboundId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        ValidateRoutingRevision(expectedRoutingRevision);
        var context = await _contextProvider.CreateAsync(
            "set-default-route",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.SetDefaultRouteAsync(
                CreateSetDefaultRouteRequest(
                    outboundId,
                    expectedRoutingRevision,
                    context),
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<SetDefaultRouteResponse> SetDirectRouteAsync(
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        ValidateRoutingRevision(expectedRoutingRevision);
        var context = await _contextProvider.CreateAsync(
            "set-direct-route",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.SetDefaultRouteAsync(
                CreateSetDirectRouteRequest(
                    expectedRoutingRevision,
                    context),
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    internal static SetDefaultRouteRequest CreateSetDefaultRouteRequest(
        string outboundId,
        ulong expectedRoutingRevision,
        OperationContext context)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        ValidateRoutingRevision(expectedRoutingRevision);
        ArgumentNullException.ThrowIfNull(context);
        return new SetDefaultRouteRequest
        {
            Context = context,
            OutboundId = outboundId,
            ExpectedRoutingRevision = expectedRoutingRevision,
        };
    }

    internal static SetDefaultRouteRequest CreateSetDirectRouteRequest(
        ulong expectedRoutingRevision,
        OperationContext context)
    {
        ValidateRoutingRevision(expectedRoutingRevision);
        ArgumentNullException.ThrowIfNull(context);
        return new SetDefaultRouteRequest
        {
            Context = context,
            Direct = true,
            ExpectedRoutingRevision = expectedRoutingRevision,
        };
    }

    private static void ValidateRoutingRevision(ulong expectedRoutingRevision)
    {
        ArgumentOutOfRangeException.ThrowIfZero(expectedRoutingRevision);
        ArgumentOutOfRangeException.ThrowIfEqual(
            expectedRoutingRevision,
            ulong.MaxValue);
    }

    internal static TestOutboundRequest CreateTestOutboundRequest(
        string outboundId,
        OperationContext context)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        ArgumentNullException.ThrowIfNull(context);
        return new TestOutboundRequest
        {
            Context = context,
            OutboundId = outboundId,
            Timeout = Duration.FromTimeSpan(OutboundProbeTimeout),
        };
    }

    public async Task<StartLearningSessionResponse> StartLearningSessionAsync(
        StartLearningSessionRequest request,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(request);
        var authenticated = request.Clone();
        authenticated.Context = await _contextProvider.CreateAsync(
            "start-learning",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.StartLearningSessionAsync(
                authenticated,
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<RecordLearningObservationResponse> RecordLearningObservationAsync(
        RecordLearningObservationRequest request,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(request);
        var authenticated = request.Clone();
        authenticated.Context = await _contextProvider.CreateAsync(
            "record-learning",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.RecordLearningObservationAsync(
                authenticated,
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<ListLearningCandidatesResponse> ListLearningCandidatesAsync(
        string sessionId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(sessionId);
        var context = await _contextProvider.CreateAsync(
            "list-learning",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.ListLearningCandidatesAsync(
                new ListLearningCandidatesRequest
                {
                    Context = context,
                    SessionId = sessionId,
                },
                ReadOptions(cancellationToken)).ResponseAsync);
    }

    public async Task<StopLearningSessionResponse> StopLearningSessionAsync(
        string sessionId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(sessionId);
        var context = await _contextProvider.CreateAsync(
            "stop-learning",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.StopLearningSessionAsync(
                new StopLearningSessionRequest
                {
                    Context = context,
                    SessionId = sessionId,
                },
                MutationOptions(cancellationToken)).ResponseAsync);
    }

    public void Dispose()
    {
        if (_channel.IsValueCreated)
        {
            _channel.Value.Dispose();
        }
    }

    private static CallOptions ReadOptions(CancellationToken cancellationToken)
    {
        return Options(ReadTimeout, cancellationToken);
    }

    private static CallOptions MutationOptions(CancellationToken cancellationToken)
    {
        return Options(MutationTimeout, cancellationToken);
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
            throw ControlRpcExceptionMapper.FromRpc(exception);
        }
        catch (HttpRequestException exception)
        {
            throw new ControlServiceException(
                "NP_CONTROL_UNAVAILABLE",
                "控制服务未启动或正在重启，请稍后重试。",
                exception);
        }
        catch (IOException exception)
        {
            throw new ControlServiceException(
                "NP_CONTROL_UNAVAILABLE",
                "无法连接本地控制套接字，请确认后台服务正在运行。",
                exception);
        }
    }
}
