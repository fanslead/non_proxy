using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Networks;

public sealed record NetworkProfileViewItem(
    NetworkProfileListItem Profile,
    IReadOnlyList<PolicyListItem> Policies)
{
    public string Id => Profile.Id;

    public string DisplayName => Profile.DisplayName;

    public string FingerprintKindLabel => Profile.FingerprintKindLabel;

    public string FingerprintPreview => Profile.FingerprintPreview;

    public string AccuracyLabel => Profile.AccuracyLabel;

    public string RuleStateLabel
    {
        get
        {
            var editable = Policies
                .Where(policy => policy.State != PolicyApplyState.PendingRemoval)
                .ToArray();
            if (editable.Length > 1)
            {
                return $"存在 {editable.Length} 条可编辑规则，请在“全部规则”中检查";
            }

            if (editable.Length == 1)
            {
                var policy = editable[0];
                var label = policy.Action == PolicyAction.Direct
                    ? $"直连规则 · {policy.StateLabel}"
                    : $"当前为{policy.ActionLabel}规则 · {policy.StateLabel}";
                return Policies.Any(item => item.State == PolicyApplyState.PendingRemoval)
                    ? $"{label}；旧规则等待移除"
                    : label;
            }

            return Policies.Count == 0
                ? "未创建直连规则"
                : "直连规则正在从数据面移除";
        }
    }
}
