using System.Security.Cryptography;
using System.Text;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed partial class GatewayOutboundService
{
    private const string ProxyUriListFormat = "proxy-uri-list-v1";
    private const string ShadowsocksSubscriptionFormat = "shadowsocks-subscription-v1";
    private const int MaximumUriListBytes = 256 * 1024;

    public Task<OutboundImportResult> PreviewUriListAsync(
        string uriList,
        CancellationToken cancellationToken)
    {
        return ImportUriListCoreAsync(uriList, true, cancellationToken);
    }

    public Task<OutboundImportResult> ImportUriListAsync(
        string uriList,
        CancellationToken cancellationToken)
    {
        return ImportUriListCoreAsync(uriList, false, cancellationToken);
    }

    private async Task<OutboundImportResult> ImportUriListCoreAsync(
        string uriList,
        bool validateOnly,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(uriList);
        var configuration = Encoding.UTF8.GetBytes(uriList);
        if (configuration.Length > MaximumUriListBytes)
        {
            CryptographicOperations.ZeroMemory(configuration);
            throw new ControlServiceException(
                "NP_REQUEST_INVALID",
                "代理链接内容不能超过 256 KiB。");
        }

        NonProxy.Control.V1.ImportConfigurationResponse response;
        try
        {
            response = await _client.ImportConfigurationAsync(
                SelectImportFormat(uriList),
                configuration,
                validateOnly,
                cancellationToken);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(configuration);
        }
        if (response.Error is { } error)
        {
            throw new ControlServiceException(
                error.Code,
                UriImportErrorMessage(error));
        }
        if (string.IsNullOrWhiteSpace(response.ImportId)
            || response.Outbounds.Count == 0)
        {
            throw new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "控制服务没有返回完整的代理链接检查结果。");
        }

        return new OutboundImportResult(
            response.ImportId,
            response.Outbounds.Select(OutboundContractMapper.ToItem).ToArray(),
            response.Warnings.ToArray());
    }

    private static string SelectImportFormat(string value)
    {
        var source = value.AsSpan().TrimStart();
        return source.IndexOf("://", StringComparison.Ordinal) >= 0
                ? ProxyUriListFormat
                : ShadowsocksSubscriptionFormat;
    }

    private static string UriImportErrorMessage(
        NonProxy.Common.V1.ErrorDetail error)
    {
        var message = ImportErrorMessage(error.Code);
        return error.Metadata.TryGetValue("line", out var line)
            && uint.TryParse(
                line,
                System.Globalization.NumberStyles.None,
                System.Globalization.CultureInfo.InvariantCulture,
                out var lineNumber)
            && lineNumber > 0
                ? $"第 {lineNumber} 行：{message}"
                : message;
    }
}
