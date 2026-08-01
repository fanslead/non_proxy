using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Activity;

public sealed class ActivityRecordViewItem
{
    private ActivityRecordViewItem(
        ActivityItem record,
        bool canPrepareDirect,
        string quickActionStatus)
    {
        Record = record;
        CanPrepareDirect = canPrepareDirect;
        QuickActionStatus = quickActionStatus;
    }

    public ActivityItem Record { get; }

    public bool CanPrepareDirect { get; }

    public string QuickActionStatus { get; }

    public bool HasQuickActionStatus => !CanPrepareDirect;

    public string Application => Record.Application;

    public string Destination => Record.Destination;

    public string ResultLabel => Record.ResultLabel;

    public string OccurredAtLabel => Record.OccurredAtLabel;

    public string Path => Record.Path;

    public string Reason => Record.Reason;

    public string SnapshotLabel => Record.SnapshotLabel;

    public string Error => Record.Error;

    public bool HasError => Record.HasError;

    public string IdentityAssuranceLabel => Record.HasSignedApplicationIdentity
        ? "已验证应用签名"
        : "未保存可验证签名";

    public static ActivityRecordViewItem Create(
        ActivityItem record,
        PlatformKind currentPlatform,
        IReadOnlySet<string> configuredApplicationIdentities)
    {
        ArgumentNullException.ThrowIfNull(record);
        ArgumentNullException.ThrowIfNull(configuredApplicationIdentities);

        var status = IneligibleStatus(
            record,
            currentPlatform,
            configuredApplicationIdentities);
        return new ActivityRecordViewItem(record, status is null, status ?? string.Empty);
    }

    private static string? IneligibleStatus(
        ActivityItem record,
        PlatformKind currentPlatform,
        IReadOnlySet<string> configuredApplicationIdentities)
    {
        if (record.IsSystemDecision)
        {
            return "系统保护流量";
        }
        if (record.ApplicationPlatform != currentPlatform)
        {
            return "其他平台记录";
        }
        if (!record.HasSignedApplicationIdentity
            || record.ApplicationStableId == "unknown-app"
            || record.ApplicationRuleStableId == "unknown-app")
        {
            return "应用身份不足";
        }
        if (configuredApplicationIdentities.Contains(record.ApplicationRuleStableId))
        {
            return "已有应用规则";
        }
        return null;
    }
}
