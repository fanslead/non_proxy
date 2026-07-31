using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control.Gateway;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayDiagnosticsServiceTests
{
    [Theory]
    [InlineData(0UL, "正常", "未检测到")]
    [InlineData(7UL, "需关注", "7 条")]
    public async Task DecisionEvidenceCheckExposesQueueLoss(
        ulong droppedEvents,
        string expectedStatus,
        string expectedDetail)
    {
        var client = new StubControlRpcClient
        {
            StatusResponse = new GetSystemStatusResponse
            {
                DroppedDecisionEvents = droppedEvents,
            },
        };
        var installer = new RecordingSystemComponentInstaller
        {
            State = new SystemComponentState(
                SystemComponentStatus.Installed,
                "系统组件已安装"),
        };
        var service = new GatewayDiagnosticsService(client, installer);

        var checks = await service.RunChecksAsync(
            TestContext.Current.CancellationToken);

        var evidence = Assert.Single(
            checks,
            check => check.Id == "decision-evidence");
        Assert.Equal(expectedStatus, evidence.Status);
        Assert.Contains(
            expectedDetail,
            evidence.Detail,
            StringComparison.Ordinal);
    }
}
