using System.Security.Cryptography;
using System.Text.Json;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayOutboundService : IOutboundService
{
    private const int MaximumPages = 100;
    private const int MaximumCredentialLength = 255;

    private readonly IControlRpcClient _client;

    public GatewayOutboundService(IControlRpcClient client)
    {
        _client = client;
    }

    public async Task<OutboundCatalog> ListAsync(
        CancellationToken cancellationToken)
    {
        var items = new List<OutboundListItem>();
        var tokens = new HashSet<string>(StringComparer.Ordinal);
        var pageToken = string.Empty;
        ulong? routingRevision = null;
        for (var page = 0; page < MaximumPages; page++)
        {
            if (!tokens.Add(pageToken))
            {
                throw InvalidPaging();
            }

            var response = await _client.ListOutboundsAsync(
                pageToken,
                cancellationToken);
            if (response.RoutingRevision == 0
                || routingRevision is not null
                    && routingRevision != response.RoutingRevision)
            {
                throw InvalidPaging();
            }

            routingRevision = response.RoutingRevision;
            items.AddRange(response.Outbounds.Select(OutboundContractMapper.ToItem));
            pageToken = response.Page?.NextPageToken ?? string.Empty;
            if (string.IsNullOrEmpty(pageToken))
            {
                if (items.Select(item => item.Id).Distinct(
                        StringComparer.Ordinal).Count() != items.Count
                    || items.Count(item => item.IsDefault) > 1)
                {
                    throw InvalidPaging();
                }

                return new OutboundCatalog(
                    items,
                    routingRevision.Value,
                    items.SingleOrDefault(item => item.IsDefault)?.Id);
            }
        }

        throw InvalidPaging();
    }

    public async Task<ApplyResult> SetDefaultAsync(
        string outboundId,
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        ValidateRoutingRevision(expectedRoutingRevision);
        var response = await _client.SetDefaultRouteAsync(
            outboundId,
            expectedRoutingRevision,
            cancellationToken);
        return MapRouteChange(
            response,
            expectedRoutingRevision,
            "默认代理已保存，新的路由快照正在等待系统组件确认。");
    }

    public async Task<ApplyResult> SetDirectAsync(
        ulong expectedRoutingRevision,
        CancellationToken cancellationToken)
    {
        ValidateRoutingRevision(expectedRoutingRevision);
        var response = await _client.SetDirectRouteAsync(
            expectedRoutingRevision,
            cancellationToken);
        return MapRouteChange(
            response,
            expectedRoutingRevision,
            "默认直连已保存，新的路由快照正在等待系统组件确认。");
    }

    private static ApplyResult MapRouteChange(
        SetDefaultRouteResponse response,
        ulong expectedRoutingRevision,
        string acceptedMessage)
    {
        if (response.Error is { } error)
        {
            return new ApplyResult(
                false,
                false,
                error.Code,
                DefaultRouteErrorMessage(error.Code),
                null);
        }
        if (response.RoutingRevision != expectedRoutingRevision + 1
            || response.Snapshot is null
            || response.Snapshot.SnapshotVersion == 0
            || response.Snapshot.State
                != NonProxy.Policy.V1.SnapshotState.PendingAck)
        {
            throw new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "控制服务没有返回完整的默认路由发布结果。");
        }

        return new ApplyResult(
            true,
            false,
            "NP_SNAPSHOT_PENDING_ACK",
            acceptedMessage,
            response.Snapshot.SnapshotVersion);
    }

    private static void ValidateRoutingRevision(ulong expectedRoutingRevision)
    {
        ArgumentOutOfRangeException.ThrowIfZero(expectedRoutingRevision);
        ArgumentOutOfRangeException.ThrowIfEqual(
            expectedRoutingRevision,
            ulong.MaxValue);
    }

    public async Task<OutboundImportResult> ImportAsync(
        OutboundImportDraft draft,
        CancellationToken cancellationToken)
    {
        Validate(draft);
        var configuration = JsonSerializer.SerializeToUtf8Bytes(new
        {
            version = 1,
            outbounds = new[]
            {
                new
                {
                    id = draft.Id.Trim(),
                    kind = KindValue(draft.Kind),
                    host = draft.Host.Trim(),
                    port = draft.Port,
                    username = EmptyToNull(draft.Username),
                    password = EmptyToNull(draft.Password),
                    enabled = true,
                },
            },
        });
        ImportConfigurationResponse response;
        try
        {
            response = await _client.ImportConfigurationAsync(
                configuration,
                cancellationToken);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(configuration);
        }
        if (response.Error is { } error)
        {
            var warning = response.Warnings.Count == 0
                ? string.Empty
                : $" {string.Join("；", response.Warnings)}";
            throw new ControlServiceException(
                error.Code,
                $"{ImportErrorMessage(error.Code)}{warning}");
        }
        if (string.IsNullOrWhiteSpace(response.ImportId)
            || response.Outbounds.Count != 1)
        {
            throw new ControlServiceException(
                "NP_CONTROL_CONTRACT_INVALID",
                "控制服务没有返回完整的代理导入结果。");
        }

        return new OutboundImportResult(
            response.ImportId,
            response.Outbounds.Select(OutboundContractMapper.ToItem).ToArray(),
            response.Warnings.ToArray());
    }

    public async Task<OutboundTestResult> TestAsync(
        string outboundId,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(outboundId);
        var response = await _client.TestOutboundAsync(
            outboundId,
            cancellationToken);
        var checkedAt = DateTimeOffset.UtcNow;
        if (response.Error is { } error)
        {
            return new OutboundTestResult(
                outboundId,
                false,
                "握手异常",
                null,
                checkedAt,
                TestErrorMessage(error.Code));
        }
        if (!response.Healthy || response.Latency is null)
        {
            throw OutboundContractMapper.InvalidProbeContract();
        }

        return new OutboundTestResult(
            outboundId,
            true,
            "代理握手可用",
            OutboundContractMapper.ToTimeSpan(response.Latency),
            checkedAt,
            "代理握手成功；该结果不代表公网出口 IP 或最终规则路径已经验证。");
    }

    private static string KindValue(OutboundProxyKind kind)
    {
        return kind switch
        {
            OutboundProxyKind.Socks5 => "socks5",
            OutboundProxyKind.HttpConnect => "http_connect",
            _ => throw new ArgumentOutOfRangeException(nameof(kind)),
        };
    }

    private static string? EmptyToNull(string? value)
    {
        return string.IsNullOrWhiteSpace(value) ? null : value;
    }

    private static void Validate(OutboundImportDraft draft)
    {
        ArgumentNullException.ThrowIfNull(draft);
        if (string.IsNullOrWhiteSpace(draft.Id)
            || string.IsNullOrWhiteSpace(draft.Host)
            || draft.Port == 0)
        {
            throw new ControlServiceException(
                "NP_REQUEST_INVALID",
                "请填写代理标识、服务器地址和有效端口。");
        }
        if ((string.IsNullOrWhiteSpace(draft.Username)
             != string.IsNullOrWhiteSpace(draft.Password))
            || draft.Username?.Length > MaximumCredentialLength
            || draft.Password?.Length > MaximumCredentialLength)
        {
            throw new ControlServiceException(
                "NP_REQUEST_INVALID",
                "账号和密码必须同时填写，且每项不能超过 255 个字符。");
        }
    }

    private static string ImportErrorMessage(string code)
    {
        return code switch
        {
            "NP_REQUEST_INVALID" or "NP_OUTBOUND_IMPORT_INVALID"
                => "代理配置无效，请检查标识、地址、端口和凭据。",
            "NP_OUTBOUND_IMPORT_DUPLICATE_ID" => "同一次导入不能包含重复的代理标识。",
            "NP_OUTBOUND_CREDENTIAL_INVALID" => "代理账号和密码必须同时填写且符合长度限制。",
            "NP_OUTBOUND_REVISION_EXHAUSTED" => "代理配置修订号已耗尽，请更换配置标识。",
            "NP_OUTBOUND_REVISION_CONFLICT" => "代理配置已被其他操作修改，请刷新后重试。",
            "NP_CREDENTIAL_STORE_FAILED" => "系统凭据库暂时无法保存代理账号和密码。",
            "NP_STORAGE_FAILURE" => "代理配置存储暂时不可用。",
            _ => "控制服务没有接受本次代理配置。",
        };
    }

    private static string TestErrorMessage(string code)
    {
        return code switch
        {
            "NP_FLOW_OUTBOUND_NOT_FOUND" => "代理配置已不存在，请刷新列表。",
            "NP_FLOW_OUTBOUND_DISABLED" => "该代理已停用，请先启用后再测试。",
            "NP_FLOW_OUTBOUND_UNSUPPORTED" => "当前代理类型暂不支持内置握手测试。",
            "NP_FLOW_OUTBOUND_INVALID" => "代理配置不完整，请重新保存。",
            "NP_FLOW_CREDENTIAL_UNAVAILABLE"
                => "无法从系统凭据库读取代理账号密码。",
            "NP_OUTBOUND_TEST_TIMEOUT"
                => "代理握手超时，请检查地址、端口和网络状态。",
            "NP_FLOW_OUTBOUND_CONNECT_FAILED" or "NP_FLOW_IO_FAILED"
                => "代理握手失败，请检查地址、端口、认证信息和代理服务状态。",
            _ => "代理握手未完成，请稍后重试。",
        };
    }

    private static string DefaultRouteErrorMessage(string code)
    {
        return code switch
        {
            "NP_ROUTING_REVISION_CONFLICT" => "默认路由已被其他操作修改，请刷新后重试。",
            "NP_DEFAULT_OUTBOUND_UNAVAILABLE"
                => "该代理已不存在、已停用或能力不足，请刷新列表。",
            "NP_SNAPSHOT_ALREADY_PENDING"
                => "已有路由快照等待系统组件确认，请稍后刷新再试。",
            "NP_POLICY_COMPILE_REJECTED"
                => "当前代理能力不足以承载所有未匹配流量，请检查代理类型和规则。",
            _ => "控制服务没有接受本次默认代理修改。",
        };
    }

    private static ControlServiceException InvalidPaging()
    {
        return new ControlServiceException(
            "NP_CONTROL_PAGING_INVALID",
            "控制服务返回了无效出口分页游标。");
    }
}
