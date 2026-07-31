using System.Runtime.CompilerServices;
using Grpc.Core;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Events;
using NonProxy.Events.V1;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient
{
    public async IAsyncEnumerable<ControlEventNotification> SubscribeAsync(
        ulong afterSequence,
        [EnumeratorCancellation] CancellationToken cancellationToken)
    {
        using var call = CreateEventCall(afterSequence, cancellationToken);
        await AwaitEventHeadersAsync(call.ResponseHeadersAsync, cancellationToken);
        yield return ControlEventNotification.Ready;

        while (await MoveNextEventAsync(call.ResponseStream, cancellationToken))
        {
            var response = call.ResponseStream.Current;
            if (response.Event is null)
            {
                throw new ControlServiceException(
                    "NP_CONTROL_EVENT_INVALID",
                    "控制服务返回了不完整的状态事件。");
            }

            yield return MapEvent(response.Event);
        }
    }

    internal static ControlEventNotification MapEvent(EventEnvelope value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new ControlEventNotification(
            value.Sequence,
            value.PayloadCase switch
            {
                EventEnvelope.PayloadOneofCase.SystemStateChanged =>
                    ControlEventKind.SystemState,
                EventEnvelope.PayloadOneofCase.SnapshotStateChanged =>
                    ControlEventKind.Snapshot,
                EventEnvelope.PayloadOneofCase.DecisionObserved =>
                    ControlEventKind.Decision,
                EventEnvelope.PayloadOneofCase.ComponentHealthChanged =>
                    ControlEventKind.ComponentHealth,
                EventEnvelope.PayloadOneofCase.LearningCandidateUpdated =>
                    ControlEventKind.LearningCandidate,
                _ => ControlEventKind.Unknown,
            });
    }

    private AsyncServerStreamingCall<SubscribeEventsResponse> CreateEventCall(
        ulong afterSequence,
        CancellationToken cancellationToken)
    {
        try
        {
            return Client.SubscribeEvents(
                new SubscribeEventsRequest
                {
                    AfterSequence = afterSequence,
                    MinimumSeverity = Severity.Info,
                },
                cancellationToken: cancellationToken);
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

    private static async Task AwaitEventHeadersAsync(
        Task<Metadata> responseHeaders,
        CancellationToken cancellationToken)
    {
        try
        {
            _ = await responseHeaders.WaitAsync(cancellationToken);
        }
        catch (RpcException) when (cancellationToken.IsCancellationRequested)
        {
            throw new OperationCanceledException(cancellationToken);
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

    private static async Task<bool> MoveNextEventAsync(
        IAsyncStreamReader<SubscribeEventsResponse> stream,
        CancellationToken cancellationToken)
    {
        try
        {
            return await stream.MoveNext(cancellationToken);
        }
        catch (RpcException) when (cancellationToken.IsCancellationRequested)
        {
            throw new OperationCanceledException(cancellationToken);
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
