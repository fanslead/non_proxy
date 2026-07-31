using NonProxy.Desktop.Core.Features.Diagnostics;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class DiagnosticsViewModelTests
{
    [Fact]
    public async Task ExportCommandPublishesLocalPreviewAndPrivacyBoundary()
    {
        var exported = new DiagnosticExport(
            "diag-a",
            "/tmp/nonproxy-diag-a.json",
            1024,
            new string('a', 64),
            "严格脱敏",
            DateTimeOffset.UtcNow.AddHours(-1),
            DateTimeOffset.UtcNow,
            ["组件版本、系统与能力"],
            0,
            2);
        var service = new StubDiagnosticsService(exported);
        var viewModel = new DiagnosticsViewModel(service);

        await viewModel.ExportCommand.ExecuteAsync(null);

        Assert.Same(exported, viewModel.LatestExport);
        Assert.True(viewModel.HasLatestExport);
        Assert.True(viewModel.HasOperationMessage);
        Assert.Contains(
            "不会自动上传",
            viewModel.OperationMessage,
            StringComparison.Ordinal);
        Assert.Equal(1, service.ExportCallCount);
    }

    private sealed class StubDiagnosticsService(DiagnosticExport exported)
        : IDiagnosticsService
    {
        public int ExportCallCount { get; private set; }

        public Task<IReadOnlyList<DiagnosticCheck>> RunChecksAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult<IReadOnlyList<DiagnosticCheck>>([]);
        }

        public Task<DiagnosticExport> ExportAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ExportCallCount++;
            return Task.FromResult(exported);
        }
    }
}
