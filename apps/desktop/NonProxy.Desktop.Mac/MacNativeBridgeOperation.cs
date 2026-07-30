using System.Runtime.InteropServices;
using System.Text;

namespace NonProxy.Desktop.Mac;

internal sealed class MacNativeBridgeOperation
{
    private const int CompletedEvent = 2;
    private const int MaximumPayloadLength = 1024 * 1024;
    private static readonly Encoding StrictUtf8 =
        new UTF8Encoding(false, true);

    private readonly ulong _operationId;
    private readonly Action? _approvalRequired;
    private readonly Action? _operationCompleted;
    private readonly TaskCompletionSource<string> _completion =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
    private CancellationTokenRegistration _cancellationRegistration;
    private nint _handlePointer;
    private int _released;

    internal MacNativeBridgeOperation(
        ulong operationId,
        Action? approvalRequired,
        Action? operationCompleted,
        CancellationToken cancellationToken)
    {
        _operationId = operationId;
        _approvalRequired = approvalRequired;
        _operationCompleted = operationCompleted;
        _cancellationRegistration = cancellationToken.Register(
            () => _completion.TrySetCanceled(cancellationToken));
    }

    internal Task<string> Completion => _completion.Task;

    internal nint AllocateHandle()
    {
        var handle = GCHandle.Alloc(this);
        _handlePointer = GCHandle.ToIntPtr(handle);
        return _handlePointer;
    }

    internal unsafe void HandleCallback(
        ulong operationId,
        int eventKind,
        int statusCode,
        byte* payload,
        nuint payloadLength)
    {
        if (operationId != _operationId)
        {
            CompleteWithError(
                "NP_MAC_BRIDGE_OPERATION_MISMATCH",
                "原生桥接返回了不匹配的操作标识。");
            return;
        }

        if (eventKind != CompletedEvent)
        {
            if (statusCode == 1)
            {
                try
                {
                    _approvalRequired?.Invoke();
                }
                catch
                {
                    // 用户界面通知失败不能越过 ABI 边界，也不能中止系统请求。
                }
            }
            return;
        }

        string json;
        try
        {
            json = CopyPayload(payload, payloadLength);
        }
        catch (Exception exception)
        {
            CompleteWithError(
                "NP_MAC_BRIDGE_INVALID_PAYLOAD",
                "原生桥接返回了无效的 UTF-8 数据。",
                exception);
            return;
        }

        _completion.TrySetResult(json);
        ReleaseHandle();
    }

    internal void CompleteStartFailure(
        string errorCode,
        string message,
        Exception? innerException = null)
    {
        CompleteWithError(errorCode, message, innerException);
    }

    private static unsafe string CopyPayload(
        byte* payload,
        nuint payloadLength)
    {
        if (payloadLength > MaximumPayloadLength)
        {
            throw new InvalidDataException("原生桥接响应超过大小上限。");
        }
        if (payloadLength == 0)
        {
            return string.Empty;
        }
        if (payload is null)
        {
            throw new InvalidDataException("原生桥接响应指针为空。");
        }

        var length = checked((int)payloadLength);
        return StrictUtf8.GetString(new ReadOnlySpan<byte>(payload, length));
    }

    private void CompleteWithError(
        string errorCode,
        string message,
        Exception? innerException = null)
    {
        _completion.TrySetException(new MacNativeBridgeException(
            errorCode,
            message,
            innerException));
        ReleaseHandle();
    }

    private void ReleaseHandle()
    {
        if (Interlocked.Exchange(ref _released, 1) == 1)
        {
            return;
        }

        _cancellationRegistration.Dispose();
        try
        {
            _operationCompleted?.Invoke();
        }
        catch
        {
            // 生命周期通知失败不能越过 ABI 边界或阻止句柄释放。
        }
        if (_handlePointer != nint.Zero)
        {
            GCHandle.FromIntPtr(_handlePointer).Free();
            _handlePointer = nint.Zero;
        }
    }
}
