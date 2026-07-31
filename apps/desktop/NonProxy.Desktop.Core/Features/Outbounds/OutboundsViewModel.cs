using System.Collections.ObjectModel;
using System.Globalization;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed partial class OutboundsViewModel : LoadableViewModel
{
    private readonly IOutboundService _outboundService;
    private readonly ILocalProxyDiscovery _localProxyDiscovery;
    private ulong _routingRevision;

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

    [ObservableProperty]
    private string _defaultRouteSummary = "配置：读取中";

    [ObservableProperty]
    private bool _exitVerificationAvailable;

    [ObservableProperty]
    private ExitVerificationReceipt? _directExitReceipt;

    private bool _usesDirectByDefault = true;

    public OutboundsViewModel(
        IOutboundService outboundService,
        ILocalProxyDiscovery localProxyDiscovery)
        : base("网络出口")
    {
        _outboundService = outboundService;
        _localProxyDiscovery = localProxyDiscovery;
        InitializeLocalProxyDiscoveryCommand();
        InitializeUriImportCommands();
        ImportCommand = new AsyncRelayCommand(ImportAsync, CanImport);
        TestCommand = new AsyncRelayCommand<OutboundListItem>(TestAsync);
        VerifyExitCommand = new AsyncRelayCommand<OutboundListItem>(
            VerifyProxyExitAsync,
            CanVerifyProxyExit);
        VerifyDirectExitCommand = new AsyncRelayCommand(
            VerifyDirectExitAsync,
            CanVerifyDirectExit);
        SetDefaultCommand = new AsyncRelayCommand<OutboundListItem>(
            SetDefaultAsync,
            CanSetDefault);
        SetDirectCommand = new AsyncRelayCommand(
            SetDirectAsync,
            CanSetDirect);
    }

    public static IReadOnlyList<OutboundKindOption> KindOptions { get; } =
    [
        new("SOCKS5", OutboundProxyKind.Socks5, "支持 TCP 和 UDP"),
        new("HTTP CONNECT", OutboundProxyKind.HttpConnect, "仅支持 TCP"),
    ];

    public ObservableCollection<OutboundListItem> Items { get; } = [];

    public IAsyncRelayCommand ImportCommand { get; }

    public IAsyncRelayCommand<OutboundListItem> TestCommand { get; }

    public IAsyncRelayCommand<OutboundListItem> VerifyExitCommand { get; }

    public IAsyncRelayCommand VerifyDirectExitCommand { get; }

    public IAsyncRelayCommand<OutboundListItem> SetDefaultCommand { get; }

    public IAsyncRelayCommand SetDirectCommand { get; }

    public string ExitVerificationAvailabilityMessage =>
        ExitVerificationAvailable
            ? "可信签名探针已就绪。验证只发送随机 nonce，不发送应用、网站或规则信息。"
            : "当前安装尚未配置可信签名探针；历史回执仍可查看，但不能发起新的公网出口验证。";

    public string DirectExitStatusLabel => DirectExitReceipt is { } value
        ? $"最近签名回执 · {value.ObservedIp}"
        : "尚无签名回执";

    public string DirectExitCheckedLabel => DirectExitReceipt is { } value
        ? value.VerifiedAt.ToLocalTime().ToString(
            "MM-dd HH:mm:ss",
            CultureInfo.CurrentCulture)
        : "—";

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

    partial void OnExitVerificationAvailableChanged(bool value)
    {
        OnPropertyChanged(nameof(ExitVerificationAvailabilityMessage));
        VerifyDirectExitCommand.NotifyCanExecuteChanged();
        VerifyExitCommand.NotifyCanExecuteChanged();
    }

    partial void OnDirectExitReceiptChanged(ExitVerificationReceipt? value)
    {
        OnPropertyChanged(nameof(DirectExitStatusLabel));
        OnPropertyChanged(nameof(DirectExitCheckedLabel));
    }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        var catalog = await _outboundService.ListAsync(cancellationToken);
        _routingRevision = catalog.RoutingRevision;
        _usesDirectByDefault = catalog.UsesDirectByDefault;
        ExitVerificationAvailable = catalog.ExitVerificationAvailable;
        DirectExitReceipt = catalog.DirectExitReceipt;
        DefaultRouteSummary = _routingRevision == 0
            ? "配置：暂时无法读取"
            : catalog.DefaultOutboundId is { } outboundId
                ? $"配置：未命中规则时使用代理 {outboundId}"
                : "配置：未命中规则时默认直连";
        Items.Clear();
        foreach (var item in catalog.Items.OrderBy(item => item.Name))
        {
            Items.Add(item);
        }
        SetDefaultCommand.NotifyCanExecuteChanged();
        SetDirectCommand.NotifyCanExecuteChanged();
        VerifyExitCommand.NotifyCanExecuteChanged();
        VerifyDirectExitCommand.NotifyCanExecuteChanged();
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
                        IsHandshakeVerified = result.Healthy,
                    };
                    SetDefaultCommand.NotifyCanExecuteChanged();
                }

                OperationMessage = result.Message;
            },
            cancellationToken);
    }

    private bool CanSetDefault(OutboundListItem? item)
    {
        return item is { CanSetAsDefault: true } && _routingRevision > 0;
    }

    private bool CanVerifyProxyExit(OutboundListItem? item)
    {
        return ExitVerificationAvailable && item is { CanVerifyExit: true };
    }

    private bool CanVerifyDirectExit()
    {
        return ExitVerificationAvailable;
    }

    private Task VerifyDirectExitAsync(CancellationToken cancellationToken)
    {
        return VerifyExitAsync(null, cancellationToken);
    }

    private Task VerifyProxyExitAsync(
        OutboundListItem? item,
        CancellationToken cancellationToken)
    {
        if (!CanVerifyProxyExit(item))
        {
            return Task.CompletedTask;
        }
        return VerifyExitAsync(item!.Id, cancellationToken);
    }

    private Task VerifyExitAsync(
        string? outboundId,
        CancellationToken cancellationToken)
    {
        if (!ExitVerificationAvailable)
        {
            return Task.CompletedTask;
        }
        return RunOperationAsync(
            async token =>
            {
                var result = await _outboundService.VerifyExitAsync(
                    outboundId,
                    token);
                OperationMessage = result.Message;
                if (result.Verified)
                {
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
    }

    private Task SetDefaultAsync(
        OutboundListItem? item,
        CancellationToken cancellationToken)
    {
        if (item is null || !item.CanSetAsDefault || _routingRevision == 0)
        {
            return Task.CompletedTask;
        }

        return RunOperationAsync(
            async token =>
            {
                var result = await _outboundService.SetDefaultAsync(
                    item.Id,
                    _routingRevision,
                    token);
                OperationMessage = result.Message;
                if (result.Accepted)
                {
                    await LoadCoreAsync(token);
                }
            },
            cancellationToken);
    }

    private bool CanSetDirect()
    {
        return !_usesDirectByDefault && _routingRevision > 0;
    }

    private Task SetDirectAsync(CancellationToken cancellationToken)
    {
        if (_usesDirectByDefault || _routingRevision == 0)
        {
            return Task.CompletedTask;
        }

        return RunOperationAsync(
            async token =>
            {
                var result = await _outboundService.SetDirectAsync(
                    _routingRevision,
                    token);
                OperationMessage = result.Message;
                if (result.Accepted)
                {
                    await LoadCoreAsync(token);
                }
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
