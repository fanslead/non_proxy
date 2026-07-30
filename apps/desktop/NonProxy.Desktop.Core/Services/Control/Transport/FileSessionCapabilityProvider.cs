namespace NonProxy.Desktop.Core.Services.Control.Transport;

public sealed class FileSessionCapabilityProvider : ISessionCapabilityProvider
{
    public const int TokenLength = 32;

    private readonly LocalControlEndpoint _endpoint;

    public FileSessionCapabilityProvider(LocalControlEndpoint endpoint)
    {
        _endpoint = endpoint;
    }

    public async Task<byte[]> ReadAsync(CancellationToken cancellationToken)
    {
        if (!_endpoint.IsConfigured
            || string.IsNullOrWhiteSpace(_endpoint.SessionCapabilityPath))
        {
            throw new ControlServiceException(
                "NP_CONTROL_UNAVAILABLE",
                "本地会话能力路径尚未配置。");
        }

        try
        {
            var information = new FileInfo(_endpoint.SessionCapabilityPath);
            if (!information.Exists || information.Length != TokenLength)
            {
                throw new ControlServiceException(
                    "NP_SESSION_TOKEN_INVALID",
                    "控制服务会话尚未就绪，请稍后重试。");
            }

            if (information.LinkTarget is not null)
            {
                throw new ControlServiceException(
                    "NP_SESSION_TOKEN_INVALID",
                    "控制服务会话文件类型无效。");
            }

            var token = new byte[TokenLength];
            await using var stream = new FileStream(
                information.FullName,
                FileMode.Open,
                FileAccess.Read,
                FileShare.Read,
                TokenLength,
                FileOptions.Asynchronous | FileOptions.SequentialScan);
            var offset = 0;
            while (offset < token.Length)
            {
                var read = await stream.ReadAsync(
                    token.AsMemory(offset),
                    cancellationToken);
                if (read == 0)
                {
                    throw new ControlServiceException(
                        "NP_SESSION_TOKEN_INVALID",
                        "控制服务会话文件不完整。");
                }

                offset += read;
            }

            if (stream.ReadByte() != -1)
            {
                throw new ControlServiceException(
                    "NP_SESSION_TOKEN_INVALID",
                    "控制服务会话文件长度无效。");
            }

            return token;
        }
        catch (ControlServiceException)
        {
            throw;
        }
        catch (IOException exception)
        {
            throw new ControlServiceException(
                "NP_SESSION_TOKEN_UNREADABLE",
                "无法读取控制服务会话，请确认后台服务正在运行。",
                exception);
        }
        catch (UnauthorizedAccessException exception)
        {
            throw new ControlServiceException(
                "NP_SESSION_TOKEN_UNREADABLE",
                "没有权限读取控制服务会话。",
                exception);
        }
    }
}
