using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
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

    [Fact]
    public async Task StrictExportMapsPreviewWithoutReadingOrUploadingTheFile()
    {
        var start = DateTimeOffset.UtcNow.AddHours(-1);
        var end = DateTimeOffset.UtcNow;
        var client = new StubControlRpcClient
        {
            ExportDiagnosticsResponse = new ExportDiagnosticsResponse
            {
                DiagnosticId = "diag-a",
                LocalPath = "/tmp/nonproxy-diag-a.json",
                SizeBytes = 2048,
                Sha256 = ByteString.CopyFrom(Enumerable.Repeat((byte)0xAB, 32).ToArray()),
                AppliedRedactionLevel = DiagnosticRedactionLevel.Strict,
                EffectiveTimeRange = new TimeRange
                {
                    Start = Timestamp.FromDateTimeOffset(start),
                    End = Timestamp.FromDateTimeOffset(end),
                },
                ConnectionSampleCount = 0,
                ErrorCount = 3,
                IncludedSections =
                {
                    "runtime",
                    "recent_errors",
                },
            },
        };
        var service = new GatewayDiagnosticsService(
            client,
            new RecordingSystemComponentInstaller());

        var exported = await service.ExportAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal("diag-a", exported.DiagnosticId);
        Assert.Equal("严格脱敏", exported.Redaction);
        Assert.Equal(0, exported.ConnectionSampleCount);
        Assert.Equal(3, exported.ErrorCount);
        Assert.Equal(string.Concat(Enumerable.Repeat("ab", 32)), exported.Sha256);
        Assert.Contains(
            exported.IncludedSections,
            section => section.Contains("组件版本", StringComparison.Ordinal));
        Assert.Contains(
            exported.IncludedSections,
            section => section.Contains("错误码", StringComparison.Ordinal));
    }

    [Fact]
    public async Task ExportRejectsAResponseThatClaimsStrictButContainsSamples()
    {
        var client = new StubControlRpcClient
        {
            ExportDiagnosticsResponse = new ExportDiagnosticsResponse
            {
                DiagnosticId = "diag-a",
                LocalPath = "/tmp/nonproxy-diag-a.json",
                SizeBytes = 1,
                Sha256 = ByteString.CopyFrom(new byte[32]),
                AppliedRedactionLevel = DiagnosticRedactionLevel.Strict,
                EffectiveTimeRange = new TimeRange
                {
                    Start = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow.AddMinutes(-1)),
                    End = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
                },
                ConnectionSampleCount = 1,
                IncludedSections = { "runtime" },
            },
        };
        var service = new GatewayDiagnosticsService(
            client,
            new RecordingSystemComponentInstaller());

        var error = await Assert.ThrowsAsync<ControlServiceException>(
            () => service.ExportAsync(TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }
}
