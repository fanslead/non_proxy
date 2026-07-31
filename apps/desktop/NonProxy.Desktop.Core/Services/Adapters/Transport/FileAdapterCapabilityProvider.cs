using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Services.Adapters.Transport;

public sealed class FileAdapterCapabilityProvider(
    LocalAdapterEndpoint endpoint) : IAdapterCapabilityProvider
{
    public const int TokenLength = 32;

    public async Task<byte[]> ReadAsync(CancellationToken cancellationToken)
    {
        if (!endpoint.IsConfigured
            || string.IsNullOrWhiteSpace(endpoint.CapabilityPath))
        {
            throw Unavailable("适配器会话能力路径尚未配置。");
        }

        try
        {
            var information = new FileInfo(endpoint.CapabilityPath);
            if (!information.Exists || information.Length != TokenLength)
            {
                throw Invalid("适配器后台会话尚未就绪，请稍后重试。");
            }
            if (information.LinkTarget is not null)
            {
                throw Invalid("适配器会话文件类型无效。");
            }

            var token = new byte[TokenLength];
            await using var stream = new FileStream(
                information.FullName,
                FileMode.Open,
                FileAccess.Read,
                FileShare.Read,
                TokenLength,
                FileOptions.Asynchronous | FileOptions.SequentialScan);
            await stream.ReadExactlyAsync(token, cancellationToken);
            if (stream.ReadByte() != -1)
            {
                throw Invalid("适配器会话文件长度无效。");
            }
            return token;
        }
        catch (ControlServiceException)
        {
            throw;
        }
        catch (EndOfStreamException exception)
        {
            throw new ControlServiceException(
                "NP_ADAPTER_SESSION_INVALID",
                "适配器会话文件不完整。",
                exception);
        }
        catch (IOException exception)
        {
            throw new ControlServiceException(
                "NP_ADAPTER_SESSION_UNREADABLE",
                "无法读取适配器后台会话，请确认系统组件正在运行。",
                exception);
        }
        catch (UnauthorizedAccessException exception)
        {
            throw new ControlServiceException(
                "NP_ADAPTER_SESSION_UNREADABLE",
                "没有权限读取适配器后台会话。",
                exception);
        }
    }

    private static ControlServiceException Invalid(string message)
    {
        return new ControlServiceException("NP_ADAPTER_SESSION_INVALID", message);
    }

    private static ControlServiceException Unavailable(string message)
    {
        return new ControlServiceException("NP_ADAPTER_UNAVAILABLE", message);
    }
}
