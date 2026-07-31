using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Diagnostics;

public sealed partial class DiagnosticsViewModel : LoadableViewModel
{
    private readonly IDiagnosticsService _diagnosticsService;

    public DiagnosticsViewModel(IDiagnosticsService diagnosticsService)
        : base("诊断")
    {
        _diagnosticsService = diagnosticsService;
        ExportCommand = new AsyncRelayCommand(ExportAsync);
    }

    [ObservableProperty]
    private DiagnosticExport? _latestExport;

    [ObservableProperty]
    private string? _operationMessage;

    public ObservableCollection<DiagnosticCheck> Checks { get; } = [];

    public IAsyncRelayCommand ExportCommand { get; }

    public bool HasLatestExport => LatestExport is not null;

    public bool HasOperationMessage => !string.IsNullOrWhiteSpace(OperationMessage);

    partial void OnLatestExportChanged(DiagnosticExport? value)
    {
        OnPropertyChanged(nameof(HasLatestExport));
    }

    partial void OnOperationMessageChanged(string? value)
    {
        OnPropertyChanged(nameof(HasOperationMessage));
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var checks = await _diagnosticsService.RunChecksAsync(cancellationToken);
        Checks.Clear();
        foreach (var check in checks)
        {
            Checks.Add(check);
        }
    }

    private Task ExportAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                LatestExport = await _diagnosticsService.ExportAsync(token);
                OperationMessage =
                    "诊断包只保存在本机，已使用严格脱敏；NonProxy 不会自动上传。";
            },
            cancellationToken);
    }
}
