using System.Text;
using Google.Protobuf.WellKnownTypes;
using NonProxy.Control.V1;

namespace NonProxy.Desktop.Core.Services.Control.Gateway;

internal static class SubscriptionContractMapper
{
    internal static readonly TimeSpan MinimumRefreshInterval = TimeSpan.FromMinutes(15);
    internal static readonly TimeSpan MaximumRefreshInterval = TimeSpan.FromDays(7);

    public static SubscriptionListItem ToItem(SubscriptionSourceSummary source)
    {
        ArgumentNullException.ThrowIfNull(source);
        try
        {
            ValidateIdentifier(source.Id);
            ValidateDisplayName(source.DisplayName);
        }
        catch (ControlServiceException exception)
        {
            throw InvalidContract(exception);
        }
        var interval = ToInterval(source.RefreshInterval);
        if (source.Revision == 0
            || source.ContentGeneration == 0
            || source.NodeCount is 0 or > 100
            || source.NextRefreshAt is null
            || source.LastAttemptedAt is null
            || source.LastSucceededAt is null
            || source.ConsecutiveFailures == 0
                && !string.IsNullOrEmpty(source.LastErrorCode)
            || source.ConsecutiveFailures > 0
                && !ValidErrorCode(source.LastErrorCode)
            || source.ConsecutiveFailures > 0 && source.LastAttemptedAt is null)
        {
            throw InvalidContract();
        }

        var nextRefreshAt = ToTimestamp(source.NextRefreshAt);
        var lastAttemptedAt = ToTimestamp(source.LastAttemptedAt);
        var lastSucceededAt = ToTimestamp(source.LastSucceededAt);
        if (lastAttemptedAt < lastSucceededAt)
        {
            throw InvalidContract();
        }

        return new SubscriptionListItem(
            source.Id,
            source.DisplayName,
            source.Enabled,
            interval,
            source.Revision,
            source.ContentGeneration,
            source.ConsecutiveFailures,
            nextRefreshAt,
            lastAttemptedAt,
            lastSucceededAt,
            string.IsNullOrEmpty(source.LastErrorCode) ? null : source.LastErrorCode,
            source.NodeCount);
    }

    public static void ValidateIdentifier(string value)
    {
        var valid = !string.IsNullOrEmpty(value)
            && Encoding.UTF8.GetByteCount(value) <= 64
            && string.Equals(value, value.Trim(), StringComparison.Ordinal)
            && value.All(character => char.IsAsciiLetterOrDigit(character)
                || character is '.' or '_' or ':' or '-');
        if (!valid)
        {
            throw InvalidRequest("订阅标识只能包含字母、数字、点、横线、下划线或冒号，且最多 64 个 UTF-8 字节。");
        }
    }

    public static void ValidateDisplayName(string value)
    {
        if (string.IsNullOrWhiteSpace(value)
            || Encoding.UTF8.GetByteCount(value) > 128
            || value.Any(char.IsControl))
        {
            throw InvalidRequest("订阅名称不能为空，且最多 128 个 UTF-8 字节。");
        }
    }

    public static void ValidateInterval(TimeSpan value)
    {
        if (value < MinimumRefreshInterval
            || value > MaximumRefreshInterval
            || value.Ticks % TimeSpan.TicksPerSecond != 0)
        {
            throw InvalidRequest("刷新间隔必须是 15 分钟到 7 天之间的整秒数。");
        }
    }

    private static TimeSpan ToInterval(Duration? value)
    {
        if (value is null || value.Nanos != 0)
        {
            throw InvalidContract();
        }

        try
        {
            var interval = value.ToTimeSpan();
            ValidateInterval(interval);
            return interval;
        }
        catch (Exception exception)
            when (exception is InvalidOperationException
                or OverflowException
                or ControlServiceException)
        {
            throw InvalidContract(exception);
        }
    }

    private static bool ValidErrorCode(string value)
    {
        return value is { Length: > 0 and <= 128 }
            && value.All(character => char.IsAsciiLetterOrDigit(character) || character == '_');
    }

    private static DateTimeOffset ToTimestamp(Timestamp value)
    {
        try
        {
            return value.ToDateTimeOffset();
        }
        catch (Exception exception)
            when (exception is InvalidOperationException or OverflowException)
        {
            throw InvalidContract(exception);
        }
    }

    private static ControlServiceException InvalidRequest(string message)
    {
        return new ControlServiceException("NP_REQUEST_INVALID", message);
    }

    private static ControlServiceException InvalidContract(Exception? inner = null)
    {
        return new ControlServiceException(
            "NP_CONTROL_CONTRACT_INVALID",
            "控制服务返回了无效的订阅状态。",
            inner);
    }
}
