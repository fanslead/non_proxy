using System.Collections.ObjectModel;
using System.Globalization;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed partial class OutboundsViewModel : LoadableViewModel
{
    private readonly IOutboundService _outboundService;

    [ObservableProperty]
    private string _outboundId = "local-proxy";

    [ObservableProperty]
    private OutboundKindOption _selectedKind = KindOptions[0];

    [ObservableProperty]
    private string _host = "127.0.0.1";

    [ObservableProperty]
    private string _port = "1080";

    [ObservableProperty]
    private string _username = string.Empty;

    [ObservableProperty]
    private string _password = string.Empty;

    [ObservableProperty]
    private string? _validationMessage;

    [ObservableProperty]
    private string? _operationMessage;

    public OutboundsViewModel(IOutboundService outboundService)
        : base("网络出口")
    {
        _outboundService = outboundService;
        ImportCommand = new AsyncRelayCommand(ImportAsync, CanImport);
        TestCommand = new AsyncRelayCommand<OutboundListItem>(TestAsync);
    }

    public static IReadOnlyList<OutboundKindOption> KindOptions { get; } =
    [
        new("SOCKS5", OutboundProxyKind.Socks5, "支持 TCP 和 UDP"),
        new("HTTP CONNECT", OutboundProxyKind.HttpConnect, "仅支持 TCP"),
    ];

    public ObservableCollection<OutboundListItem> Items { get; } = [];

    public IAsyncRelayCommand ImportCommand { get; }

    public IAsyncRelayCommand<OutboundListItem> TestCommand { get; }

    partial void OnOutboundIdChanged(string value)
    {
        InputChanged();
    }

    partial void OnHostChanged(string value)
    {
        InputChanged();
    }

    partial void OnPortChanged(string value)
    {
        InputChanged();
    }

    partial void OnUsernameChanged(string value)
    {
        InputChanged();
    }

    partial void OnPasswordChanged(string value)
    {
        InputChanged();
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var items = await _outboundService.ListAsync(cancellationToken);
        Items.Clear();
        foreach (var item in items.OrderBy(item => item.Name))
        {
            Items.Add(item);
        }
    }

    private bool CanImport()
    {
        return !IsBusy
            && !string.IsNullOrWhiteSpace(OutboundId)
            && !string.IsNullOrWhiteSpace(Host)
            && !string.IsNullOrWhiteSpace(Port);
    }

    private Task ImportAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                if (!uint.TryParse(
                        Port,
                        NumberStyles.None,
                        CultureInfo.InvariantCulture,
                        out var port)
                    || port is 0 or > 65535)
                {
                    ValidationMessage = "端口必须是 1 到 65535 之间的整数。";
                    return;
                }
                if (string.IsNullOrWhiteSpace(Username)
                    != string.IsNullOrWhiteSpace(Password))
                {
                    ValidationMessage = "账号和密码需要同时填写，或同时留空。";
                    return;
                }

                var result = await _outboundService.ImportAsync(
                    new OutboundImportDraft(
                        OutboundId.Trim(),
                        SelectedKind.Kind,
                        Host.Trim(),
                        port,
                        EmptyToNull(Username),
                        EmptyToNull(Password)),
                    token);
                Password = string.Empty;
                OperationMessage = result.Warnings.Count == 0
                    ? "代理配置已安全保存。"
                    : $"代理配置已保存；{string.Join("；", result.Warnings)}";
                await LoadCoreAsync(token);
            },
            cancellationToken);
    }

    private Task TestAsync(
        OutboundListItem? item,
        CancellationToken cancellationToken)
    {
        if (item is null)
        {
            return Task.CompletedTask;
        }

        return RunOperationAsync(
            async token =>
            {
                var result = await _outboundService.TestAsync(item.Id, token);
                var index = Items.IndexOf(item);
                if (index >= 0)
                {
                    Items[index] = item with
                    {
                        Health = result.Health,
                        Latency = result.Latency,
                        LastCheckedAt = result.CheckedAt,
                    };
                }

                OperationMessage = result.Message;
            },
            cancellationToken);
    }

    private void InputChanged()
    {
        ValidationMessage = null;
        ImportCommand.NotifyCanExecuteChanged();
    }

    private static string? EmptyToNull(string value)
    {
        return string.IsNullOrWhiteSpace(value) ? null : value;
    }
}

public sealed record OutboundKindOption(
    string DisplayName,
    OutboundProxyKind Kind,
    string CapabilityHint);
