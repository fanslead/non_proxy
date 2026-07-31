using Grpc.Core;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Services.Adapters.Rpc;

internal static class AdapterRpcExceptionMapper
{
    internal static ControlServiceException FromRpc(RpcException exception)
    {
        return exception.StatusCode switch
        {
            StatusCode.Unavailable => Failure(
                "NP_ADAPTER_UNAVAILABLE",
                "适配器后台未启动或正在重启，请稍后重试。",
                exception),
            StatusCode.DeadlineExceeded => Failure(
                "NP_ADAPTER_TIMEOUT",
                "第三方客户端检查超时，请确认客户端可以正常运行。",
                exception),
            StatusCode.PermissionDenied or StatusCode.Unauthenticated => Failure(
                "NP_ADAPTER_SESSION_EXPIRED",
                "适配器后台会话已经更新，请重试本次操作。",
                exception),
            StatusCode.InvalidArgument => Failure(
                "NP_ADAPTER_REQUEST_INVALID",
                "适配器后台拒绝了无效请求，请检查所选路径。",
                exception),
            StatusCode.Cancelled => Failure(
                "NP_ADAPTER_INTERRUPTED",
                "适配器后台连接已中断。",
                exception),
            _ => Failure(
                "NP_ADAPTER_RPC_FAILED",
                "适配器后台通信失败，请打开“诊断”查看系统组件状态。",
                exception),
        };
    }

    private static ControlServiceException Failure(
        string code,
        string message,
        Exception exception)
    {
        return new ControlServiceException(code, message, exception);
    }
}
