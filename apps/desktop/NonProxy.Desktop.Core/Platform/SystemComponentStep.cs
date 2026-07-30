namespace NonProxy.Desktop.Core.Platform;

public sealed record SystemComponentStep(
    string Id,
    string Name,
    SystemComponentStepStatus Status,
    string Detail)
{
    public string StatusLabel => Status switch
    {
        SystemComponentStepStatus.Ready => "已就绪",
        SystemComponentStepStatus.AwaitingApproval => "等待允许",
        SystemComponentStepStatus.NeedsRepair => "需要修复",
        SystemComponentStepStatus.Unavailable => "不可用",
        _ => "尚未安装",
    };

    public bool IsReady => Status == SystemComponentStepStatus.Ready;

    public bool IsAttention =>
        Status is SystemComponentStepStatus.AwaitingApproval
            or SystemComponentStepStatus.NotInstalled;

    public bool IsFailed =>
        Status is SystemComponentStepStatus.NeedsRepair
            or SystemComponentStepStatus.Unavailable;
}
