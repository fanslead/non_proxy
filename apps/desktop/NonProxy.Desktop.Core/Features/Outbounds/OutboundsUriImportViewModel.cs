using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed partial class OutboundsViewModel
{
    private bool _isUriImportBusy;
    private string? _previewedUriImportText;

    [ObservableProperty]
    private string _uriImportText = string.Empty;

    [ObservableProperty]
    private string? _uriImportMessage;

    public ObservableCollection<OutboundListItem> UriImportPreview { get; } = [];

    public IAsyncRelayCommand PreviewUriImportCommand { get; private set; } = null!;

    public IAsyncRelayCommand SaveUriImportCommand { get; private set; } = null!;

    public bool HasUriImportPreview => UriImportPreview.Count > 0;

    partial void OnUriImportTextChanged(string value)
    {
        _previewedUriImportText = null;
        UriImportPreview.Clear();
        UriImportMessage = null;
        LocalProxyDiscoveryMessage = null;
        OnPropertyChanged(nameof(HasUriImportPreview));
        PreviewUriImportCommand.NotifyCanExecuteChanged();
        SaveUriImportCommand.NotifyCanExecuteChanged();
    }

    private void InitializeUriImportCommands()
    {
        PreviewUriImportCommand = new AsyncRelayCommand(
            PreviewUriImportAsync,
            CanPreviewUriImport);
        SaveUriImportCommand = new AsyncRelayCommand(
            SaveUriImportAsync,
            CanSaveUriImport);
    }

    private bool CanPreviewUriImport()
    {
        return !IsBusy
            && !_isUriImportBusy
            && !string.IsNullOrWhiteSpace(UriImportText);
    }

    private bool CanSaveUriImport()
    {
        return !IsBusy
            && !_isUriImportBusy
            && HasUriImportPreview
            && string.Equals(
                _previewedUriImportText,
                UriImportText,
                StringComparison.Ordinal);
    }

    private async Task PreviewUriImportAsync(CancellationToken cancellationToken)
    {
        if (IsBusy || _isUriImportBusy)
        {
            return;
        }

        SetUriImportBusy(true);
        try
        {
            await RunOperationAsync(
                async token =>
                {
                    var source = UriImportText;
                    ReplaceUriPreview([]);
                    UriImportMessage = null;
                    var result = await _outboundService.PreviewUriListAsync(
                        source,
                        token);
                    if (!string.Equals(UriImportText, source, StringComparison.Ordinal))
                    {
                        return;
                    }

                    _previewedUriImportText = source;
                    ReplaceUriPreview(result.Outbounds);
                    UriImportMessage = result.Warnings.Count == 0
                        ? $"已识别 {result.Outbounds.Count} 个代理；预览不会保存配置。"
                        : $"已识别 {result.Outbounds.Count} 个代理；{string.Join("；", result.Warnings)}";
                },
                cancellationToken);
        }
        finally
        {
            SetUriImportBusy(false);
        }
    }

    private async Task SaveUriImportAsync(CancellationToken cancellationToken)
    {
        if (IsBusy || _isUriImportBusy)
        {
            return;
        }

        SetUriImportBusy(true);
        try
        {
            await RunOperationAsync(
                async token =>
                {
                    var source = _previewedUriImportText;
                    if (source is null
                        || !string.Equals(
                            UriImportText,
                            source,
                            StringComparison.Ordinal))
                    {
                        return;
                    }

                    var result = await _outboundService.ImportUriListAsync(
                        source,
                        token);
                    var importedCount = result.Outbounds.Count;
                    var sourceUnchanged = string.Equals(
                        UriImportText,
                        source,
                        StringComparison.Ordinal);
                    if (sourceUnchanged)
                    {
                        UriImportText = string.Empty;
                    }
                    UriImportMessage = result.Warnings.Count == 0
                        ? $"已安全保存 {importedCount} 个代理，凭据已转存到系统凭据库。"
                        : $"已保存 {importedCount} 个代理；{string.Join("；", result.Warnings)}";
                    if (!sourceUnchanged)
                    {
                        UriImportMessage += " 当前输入已变化，因此没有自动清空。";
                    }
                    await LoadCoreAsync(token);
                },
                cancellationToken);
        }
        finally
        {
            SetUriImportBusy(false);
        }
    }

    private void ReplaceUriPreview(IReadOnlyList<OutboundListItem> values)
    {
        UriImportPreview.Clear();
        foreach (var value in values)
        {
            UriImportPreview.Add(value);
        }
        OnPropertyChanged(nameof(HasUriImportPreview));
        SaveUriImportCommand.NotifyCanExecuteChanged();
    }

    private void SetUriImportBusy(bool value)
    {
        _isUriImportBusy = value;
        PreviewUriImportCommand.NotifyCanExecuteChanged();
        SaveUriImportCommand.NotifyCanExecuteChanged();
    }
}
