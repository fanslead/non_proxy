using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Desktop.Core.Features.Diagnostics;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Tests;

public sealed class DiagnosticsViewTests
{
    [AvaloniaFact]
    public Task MacCompositionRendersStrictExportPreview()
    {
        return AssertExportPreviewAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsCompositionRendersStrictExportPreview()
    {
        return AssertExportPreviewAsync(PlatformKind.Windows, "Windows");
    }

    private static async Task AssertExportPreviewAsync(
        PlatformKind platform,
        string displayName)
    {
        var exported = new DiagnosticExport(
            "diag-a",
            "/tmp/nonproxy-diag-a.json",
            2048,
            new string('a', 64),
            "严格脱敏",
            DateTimeOffset.UtcNow.AddHours(-1),
            DateTimeOffset.UtcNow,
            ["组件版本、系统与能力", "最近稳定错误码"],
            0,
            2);
        using var services = TestPlatformServices.Create(
            platform,
            displayName,
            registrations => registrations.AddSingleton<IDiagnosticsService>(
                new VisibleDiagnosticsService(exported)));
        var viewModel = services.GetRequiredService<DiagnosticsViewModel>();
        var view = new DiagnosticsView { DataContext = viewModel };
        var window = new Window
        {
            Width = 1_200,
            Height = 900,
            Content = view,
        };

        try
        {
            window.Show();
            Dispatcher.UIThread.RunJobs();
            var button = view.FindControl<Button>("ExportDiagnosticsButton");
            var preview = view.FindControl<Border>("DiagnosticExportPreview");
            Assert.True(button?.IsEnabled);
            Assert.False(preview?.IsVisible);

            await viewModel.ExportCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            var path = view.FindControl<TextBox>("DiagnosticLocalPath");
            Assert.True(preview?.IsVisible);
            Assert.True(path?.IsReadOnly);
            Assert.Equal(exported.LocalPath, path?.Text);
        }
        finally
        {
            window.Close();
        }
    }

    private sealed class VisibleDiagnosticsService(DiagnosticExport exported)
        : IDiagnosticsService
    {
        public Task<IReadOnlyList<DiagnosticCheck>> RunChecksAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult<IReadOnlyList<DiagnosticCheck>>([]);
        }

        public Task<DiagnosticExport> ExportAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(exported);
        }
    }
}
