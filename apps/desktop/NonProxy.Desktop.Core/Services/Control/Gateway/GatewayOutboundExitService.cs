using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed partial class GatewayOutboundService
{
    private const ulong MaximumExitReceipts = 2_048;

    private async Task<ExitReceiptCatalog> LoadExitProbesAsync(
        CancellationToken cancellationToken)
    {
        var receipts = new List<ExitVerificationReceipt>();
        var sequences = new HashSet<ulong>();
        var tokens = new HashSet<string>(StringComparer.Ordinal);
        var pageToken = string.Empty;
        ulong? totalCount = null;
        bool? available = null;
        for (var page = 0; page < MaximumPages; page++)
        {
            if (!tokens.Add(pageToken))
            {
                throw InvalidExitPaging();
            }
            var response = await _client.ListExitProbesAsync(
                200,
                pageToken,
                cancellationToken);
            if (response.TotalCount > MaximumExitReceipts
                || totalCount is not null && totalCount != response.TotalCount
                || available is not null
                    && available != response.VerificationAvailable)
            {
                throw InvalidExitPaging();
            }
            totalCount = response.TotalCount;
            available = response.VerificationAvailable;
            foreach (var probe in response.Probes)
            {
                if (!sequences.Add(probe.Sequence))
                {
                    throw InvalidExitPaging();
                }
                receipts.Add(OutboundContractMapper.ToExitReceipt(probe));
            }
            pageToken = response.Page?.NextPageToken ?? string.Empty;
            if (string.IsNullOrEmpty(pageToken))
            {
                if ((ulong)receipts.Count != totalCount)
                {
                    throw InvalidExitPaging();
                }
                return new ExitReceiptCatalog(
                    available.GetValueOrDefault(),
                    receipts.OrderByDescending(value => value.Sequence).ToArray());
            }
        }
        throw InvalidExitPaging();
    }

    private static OutboundListItem[] AttachExitReceipts(
        IReadOnlyList<OutboundListItem> items,
        IReadOnlyList<ExitVerificationReceipt> receipts)
    {
        var latestByOutbound = receipts
            .Where(receipt => receipt.OutboundId is not null)
            .GroupBy(receipt => receipt.OutboundId!, StringComparer.Ordinal)
            .ToDictionary(
                group => group.Key,
                group => group.MaxBy(receipt => receipt.Sequence),
                StringComparer.Ordinal);
        return items
            .Select(item => item with
            {
                ExitReceipt = latestByOutbound.GetValueOrDefault(item.Id),
            })
            .ToArray();
    }

    public async Task<ExitVerificationResult> VerifyExitAsync(
        string? outboundId,
        CancellationToken cancellationToken)
    {
        if (outboundId is not null)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        }
        var response = await _client.VerifyExitAsync(
            outboundId,
            cancellationToken);
        if (response.Error is { } error)
        {
            return new ExitVerificationResult(
                false,
                error.Code,
                ExitErrorMessage(error.Code));
        }
        var expectedRoute = outboundId is null
            ? ExitProbeRouteKind.Direct
            : ExitProbeRouteKind.Proxy;
        if (!response.Verified
            || response.ProbeId.Length != 43
            || !System.Net.IPAddress.TryParse(response.ObservedIp, out var address)
            || response.ObservedAt is null
            || response.IpFamily != OutboundContractMapper.ToIpFamily(address)
            || response.Route != expectedRoute
            || expectedRoute == ExitProbeRouteKind.Direct
                && !string.IsNullOrEmpty(response.OutboundId)
            || expectedRoute == ExitProbeRouteKind.Proxy
                && !string.Equals(
                    response.OutboundId,
                    outboundId,
                    StringComparison.Ordinal))
        {
            throw OutboundContractMapper.InvalidExitContract();
        }
        try
        {
            _ = response.ObservedAt.ToDateTimeOffset();
        }
        catch (InvalidOperationException exception)
        {
            throw OutboundContractMapper.InvalidExitContract(exception);
        }
        return new ExitVerificationResult(
            true,
            "NP_EXIT_PROBE_VERIFIED",
            expectedRoute == ExitProbeRouteKind.Direct
                ? $"直连公网出口已签名验证：{response.ObservedIp}"
                : $"代理公网出口已签名验证：{response.ObservedIp}");
    }

    private static string ExitErrorMessage(string code)
    {
        return code switch
        {
            "NP_EXIT_PROBE_NOT_CONFIGURED"
                => "当前安装尚未配置可信出口探针。",
            "NP_EXIT_PROBE_SYSTEM_SNAPSHOT_PENDING"
                => "防回环系统规则仍在激活中，请稍后重试。",
            "NP_EXIT_PROBE_PHYSICAL_INTERFACE_UNAVAILABLE"
                => "没有可用的物理网络接口，无法验证直连出口。",
            "NP_EXIT_PROBE_DIRECT_CONNECT_FAILED"
                => "物理直连无法连接可信出口探针，请检查网络后重试。",
            "NP_FLOW_OUTBOUND_NOT_FOUND" or "NP_FLOW_OUTBOUND_DISABLED"
                => "代理配置已不存在或已停用，请刷新列表。",
            "NP_FLOW_OUTBOUND_UNSUPPORTED" or "NP_FLOW_OUTBOUND_INVALID"
                => "当前代理配置不支持公网出口验证。",
            "NP_FLOW_CREDENTIAL_UNAVAILABLE"
                => "无法从系统凭据库读取代理账号密码。",
            "NP_FLOW_OUTBOUND_CONNECT_FAILED" or "NP_FLOW_IO_FAILED"
                => "代理无法连接可信出口探针，请检查代理网络后重试。",
            "NP_FLOW_GATEWAY_UNAVAILABLE"
                => "暂时无法读取代理配置，请稍后重试。",
            "NP_EXIT_PROBE_TIMEOUT" or "NP_EXIT_PROBE_REMOTE_UNAVAILABLE"
                => "可信出口探针暂时不可用，请检查网络后重试。",
            "NP_EXIT_PROBE_TLS_INVALID" or "NP_EXIT_PROBE_SIGNATURE_INVALID"
                => "可信出口探针身份验证失败，请停止重试并检查安装配置。",
            "NP_EXIT_PROBE_RECEIPT_EXPIRED"
                => "签名回执时间无效，请检查系统时间。",
            "NP_EXIT_PROBE_ADDRESS_INVALID"
                => "探针没有观察到可验证的公网地址。",
            "NP_EXIT_PROBE_RESPONSE_INVALID"
                => "可信出口探针返回了无效签名回执，请检查部署配置。",
            "NP_EXIT_PROBE_PERSIST_FAILED"
                => "出口已到达探针，但本地回执保存失败，请稍后重试。",
            "NP_CLOCK_INVALID"
                => "系统时间无效，无法验证签名回执。",
            _ => "公网出口验证未完成，请稍后重试。",
        };
    }

    private static ControlServiceException InvalidExitPaging()
    {
        return new ControlServiceException(
            "NP_CONTROL_PAGING_INVALID",
            "出口回执分页结果不一致，请刷新后重试。");
    }

    private sealed record ExitReceiptCatalog(
        bool Available,
        IReadOnlyList<ExitVerificationReceipt> Receipts);
}
