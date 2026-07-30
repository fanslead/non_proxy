using Grpc.Core;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public static class ControlRpcExceptionMapper
{
    public static ControlServiceException FromRpc(RpcException exception)
    {
        ArgumentNullException.ThrowIfNull(exception);
        return exception.StatusCode switch
        {
            StatusCode.Unavailable => new ControlServiceException(
                "NP_CONTROL_UNAVAILABLE",
                "控制服务未启动或正在重启，请稍后重试。",
                exception),
            StatusCode.DeadlineExceeded => new ControlServiceException(
                "NP_CONTROL_TIMEOUT",
                "控制服务响应超时，请打开“诊断”检查后台状态。",
                exception),
            StatusCode.PermissionDenied or StatusCode.Unauthenticated =>
                new ControlServiceException(
                    "NP_CONTROL_SESSION_EXPIRED",
                    "控制服务会话已经更新，请重试本次操作。",
                    exception),
            StatusCode.InvalidArgument => new ControlServiceException(
                "NP_CONTROL_REQUEST_INVALID",
                "控制服务拒绝了无效请求，请检查输入。",
                exception),
            StatusCode.Cancelled => new ControlServiceException(
                "NP_CONTROL_INTERRUPTED",
                "控制服务连接已中断。",
                exception),
            _ => new ControlServiceException(
                "NP_CONTROL_RPC_FAILED",
                "控制服务通信失败，请打开“诊断”查看后台状态。",
                exception),
        };
    }
}
