using System.Security.Cryptography;
using System.Text.Json;
using Google.Protobuf.WellKnownTypes;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Events.V1;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

public sealed class GatewayOutboundService : IOutboundService
{
    private const int MaximumPages = 100;
    private const int MaximumCredentialLength = 255;
    private static readonly TimeSpan MaximumProbeLatency = TimeSpan.FromSeconds(30);

    private readonly IControlRpcClient _client;

    public GatewayOutboundService(IControlRpcClient client)
    {
        _client = client;
    }

    public async Task<IReadOnlyList<OutboundListItem>> ListAsync(
        CancellationToken cancellationToken)
    {
        var items = new List<OutboundListItem>();
        var tokens = new HashSet<string>(StringComparer.Ordinal);
        var pageToken = string.Empty;
        for (var page = 0; page < MaximumPages; page++)
        {
            if (!tokens.Add(pageToken))
            {
                throw InvalidPaging();
            }

            var response = await _client.ListOutboundsAsync(
                pageToken,
                cancellationToken);
            items.AddRange(response.Outbounds.Select(ToItem));
            pageToken = response.Page?.NextPageToken ?? string.Empty;
            if (string.IsNullOrEmpty(pageToken))
            {
                return items;
            }
        }

        throw InvalidPaging();
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
            response.Outbounds.Select(ToItem).ToArray(),
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
            throw InvalidProbeContract();
        }

        return new OutboundTestResult(
            outboundId,
            true,
            "代理握手可用",
            ToTimeSpan(response.Latency),
            checkedAt,
            "代理握手成功；该结果不代表公网出口 IP 或最终规则路径已经验证。");
    }

    private static OutboundListItem ToItem(OutboundSummary outbound)
    {
        return new OutboundListItem(
            outbound.Id,
            string.IsNullOrWhiteSpace(outbound.DisplayName)
                ? outbound.Id
                : outbound.DisplayName,
            KindLabel(outbound.Kind),
            EndpointLabel(outbound.EndpointHost, outbound.EndpointPort),
            HealthLabel(outbound.Health),
            outbound.Latency is null ? null : ToTimeSpan(outbound.Latency),
            outbound.LastCheckedAt is null
                ? null
                : ToDateTimeOffset(outbound.LastCheckedAt));
    }

    private static string EndpointLabel(string host, uint port)
    {
        return string.IsNullOrWhiteSpace(host) || port == 0
            ? "由本地适配器管理"
            : $"{host}:{port}";
    }

    private static string KindLabel(OutboundKind kind)
    {
        return kind switch
        {
            OutboundKind.Direct => "直连",
            OutboundKind.HttpConnect => "HTTP CONNECT",
            OutboundKind.Socks5 => "SOCKS5",
            OutboundKind.Wireguard => "WireGuard",
            OutboundKind.Openvpn => "OpenVPN",
            OutboundKind.ExternalAdapter => "外部适配器",
            _ => "未知",
        };
    }

    private static string HealthLabel(RuntimeState state)
    {
        return state switch
        {
            RuntimeState.Ready => "代理握手可用",
            RuntimeState.Degraded => "握手降级",
            RuntimeState.Starting => "检测中",
            RuntimeState.Failed => "握手异常",
            RuntimeState.Stopped => "未验证",
            _ => "未验证",
        };
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

    private static TimeSpan ToTimeSpan(Duration value)
    {
        try
        {
            var result = value.ToTimeSpan();
            if (result < TimeSpan.Zero || result > MaximumProbeLatency)
            {
                throw InvalidProbeContract();
            }

            return result;
        }
        catch (InvalidOperationException exception)
        {
            throw InvalidProbeContract(exception);
        }
    }

    private static DateTimeOffset ToDateTimeOffset(Timestamp value)
    {
        try
        {
            return value.ToDateTimeOffset();
        }
        catch (InvalidOperationException exception)
        {
            throw InvalidProbeContract(exception);
        }
    }

    private static ControlServiceException InvalidProbeContract(
        Exception? innerException = null)
    {
        return new ControlServiceException(
            "NP_CONTROL_CONTRACT_INVALID",
            "控制服务返回了无效的代理测试结果。",
            innerException);
    }

    private static ControlServiceException InvalidPaging()
    {
        return new ControlServiceException(
            "NP_CONTROL_PAGING_INVALID",
            "控制服务返回了无效出口分页游标。");
    }
}
