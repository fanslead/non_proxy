using System.Security.Cryptography;
using System.Text.Json;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Events.V1;

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

    private static OutboundListItem ToItem(OutboundSummary outbound)
    {
        return new OutboundListItem(
            outbound.Id,
            outbound.DisplayName,
            KindLabel(outbound.Kind),
            EndpointLabel(outbound.EndpointHost, outbound.EndpointPort),
            HealthLabel(outbound.Health),
            null);
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
            RuntimeState.Ready => "可用",
            RuntimeState.Degraded => "降级",
            RuntimeState.Starting => "启动中",
            RuntimeState.Failed => "异常",
            RuntimeState.Stopped => "未启动",
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

    private static ControlServiceException InvalidPaging()
    {
        return new ControlServiceException(
            "NP_CONTROL_PAGING_INVALID",
            "控制服务返回了无效出口分页游标。");
    }
}
