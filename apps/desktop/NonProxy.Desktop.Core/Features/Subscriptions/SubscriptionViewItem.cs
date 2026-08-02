using System.Globalization;
using CommunityToolkit.Mvvm.ComponentModel;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Subscriptions;

public sealed partial class SubscriptionViewItem : ObservableObject
{
    [ObservableProperty]
    private bool _isDeletePending;

    public SubscriptionViewItem(SubscriptionListItem source)
    {
        Source = source;
    }

    public SubscriptionListItem Source { get; }

    public string Id => Source.Id;

    public string DisplayName => Source.DisplayName;

    public bool Enabled => Source.Enabled;

    public ulong Revision => Source.Revision;

    public bool IsHealthy => Enabled && Source.ConsecutiveFailures == 0;

    public bool NeedsAttention => Enabled && Source.ConsecutiveFailures > 0;

    public bool IsDisabled => !Enabled;

    public string StatusLabel => (Enabled, Source.ConsecutiveFailures) switch
    {
        (false, _) => "已停用",
        (true, > 0) => "刷新异常",
        _ => "同步正常",
    };

    public string StatusDetail => (Enabled, Source.ConsecutiveFailures) switch
    {
        (false, _) => $"保留 {Source.NodeCount} 个节点，不参与自动刷新",
        (true, > 0) => $"连续失败 {Source.ConsecutiveFailures} 次 · {ErrorLabel(Source.LastErrorCode)}",
        _ => $"已安全同步 {Source.NodeCount} 个节点",
    };

    public string ScheduleLabel => Enabled
        ? $"下次刷新 · {LocalTime(Source.NextRefreshAt)}"
        : "重新启用后会立即检查";

    public string LastSuccessLabel => Source.LastSucceededAt is { } value
        ? $"最近成功 · {LocalTime(value)}"
        : "尚无成功记录";

    public string IntervalLabel => FormatInterval(Source.RefreshInterval);

    public string GenerationLabel => $"内容版本 {Source.ContentGeneration}";

    public string ToggleActionLabel => Enabled ? "停用" : "重新启用";

    private static string LocalTime(DateTimeOffset value)
    {
        return value.ToLocalTime().ToString(
            "MM-dd HH:mm",
            CultureInfo.CurrentCulture);
    }

    private static string FormatInterval(TimeSpan value)
    {
        if (value.TotalDays >= 1 && value.TotalDays == Math.Truncate(value.TotalDays))
        {
            return $"每 {value.TotalDays:0} 天";
        }
        if (value.TotalHours >= 1 && value.TotalHours == Math.Truncate(value.TotalHours))
        {
            return $"每 {value.TotalHours:0} 小时";
        }
        return $"每 {value.TotalMinutes:0} 分钟";
    }

    private static string ErrorLabel(string? code)
    {
        return code switch
        {
            "NP_SUBSCRIPTION_TIMEOUT" => "连接超时",
            "NP_SUBSCRIPTION_RESOLVE_FAILED" => "域名解析失败",
            "NP_SUBSCRIPTION_CONNECT_FAILED" => "服务器连接失败",
            "NP_SUBSCRIPTION_TLS_FAILED" => "HTTPS 证书验证失败",
            "NP_SUBSCRIPTION_HTTP_STATUS_INVALID" => "订阅已失效或服务端拒绝",
            "NP_SUBSCRIPTION_CONTENT_INVALID" => "返回内容无法识别",
            "NP_CREDENTIAL_STORE_FAILED" => "系统凭据库暂时不可用",
            _ => "等待下一次重试",
        };
    }
}
