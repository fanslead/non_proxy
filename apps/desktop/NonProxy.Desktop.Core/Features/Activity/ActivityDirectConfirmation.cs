namespace NonProxy.Desktop.Core.Features.Activity;

public sealed record ActivityDirectConfirmation(
    string Application,
    string RuleStableId,
    string SignerId,
    bool IncludeHelpers)
{
    public string Title => $"让“{Application}”始终直连？";

    public string Detail => IncludeHelpers
        ? $"确认后，“{Application}”及其辅助进程之后建立的新连接将使用物理网络直连；已有连接不会被重建。"
        : $"确认后，“{Application}”之后建立的新连接将使用物理网络直连；已有连接不会被重建。";

    public string ActivationDetail =>
        $"“{Application}”的规则会等待系统网络组件确认；确认前不会显示为已生效。";

    public string IdentityLabel => $"已验证应用身份：{SignerId} · {RuleStableId}";
}
