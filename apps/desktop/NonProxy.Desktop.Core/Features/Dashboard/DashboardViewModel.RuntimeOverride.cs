using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Dashboard;

public sealed partial class DashboardViewModel
{
    private OptionalRead<RuntimeOverrideStatus> _lastRuntimeOverride =
        OptionalRead<RuntimeOverrideStatus>.Unavailable;
    private CancellationTokenSource? _runtimeOverrideExpiryRefresh;

    [ObservableProperty]
    private RuntimeOverridePanelState _runtimeOverride =
        RuntimeOverridePanelState.Loading;

    [ObservableProperty]
    private RuntimeOverrideConfirmation? _emergencyConfirmation;

    public IAsyncRelayCommand RefreshRuntimeCommand { get; }

    public IRelayCommand RequestPauseCommand { get; }

    public IRelayCommand RequestDirectOverrideCommand { get; }

    public IRelayCommand RequestProxyOverrideCommand { get; }

    public IRelayCommand CancelRuntimeOverrideCommand { get; }

    public IAsyncRelayCommand ConfirmRuntimeOverrideCommand { get; }

    public IAsyncRelayCommand ClearRuntimeOverrideCommand { get; }

    public bool IsEmergencyConfirmationVisible => EmergencyConfirmation is not null;

    partial void OnEmergencyConfirmationChanged(RuntimeOverrideConfirmation? value)
    {
        OnPropertyChanged(nameof(IsEmergencyConfirmationVisible));
    }

    public void RequestRuntimeOverride(RuntimeOverrideKind? kind)
    {
        if (kind is not { } value || !RuntimeOverride.CanRequest)
        {
            return;
        }
        try
        {
            EmergencyConfirmation = RuntimeOverrideConfirmation.Create(
                value,
                _lastOutbounds.Value?.DefaultOutboundId);
        }
        catch (ControlServiceException exception)
        {
            ErrorMessage = exception.UserMessage;
        }
    }

    private Task ConfirmRuntimeOverrideAsync(CancellationToken cancellationToken)
    {
        var confirmation = EmergencyConfirmation;
        EmergencyConfirmation = null;
        if (confirmation is null)
        {
            return Task.CompletedTask;
        }
        return RunOperationAsync(
            async token =>
            {
                var result = await _runtimeOverrideService.SetAsync(
                    confirmation.Kind,
                    confirmation.OutboundId,
                    TimeSpan.FromMinutes(5),
                    token);
                OperationMessage = result.Message;
                await LoadCoreAsync(token);
            },
            cancellationToken);
    }

    private Task ClearRuntimeOverrideAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                var result = await _runtimeOverrideService.ClearAsync(token);
                OperationMessage = result.Message;
                await LoadCoreAsync(token);
            },
            cancellationToken);
    }

    private Task RefreshRuntimeAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token =>
            {
                var overviewTask = _statusService.GetOverviewAsync(token);
                var runtimeOverrideTask = TryReadAsync(
                    _runtimeOverrideService.GetStatusAsync,
                    token);
                await Task.WhenAll(overviewTask, runtimeOverrideTask);
                _lastRuntimeOverride = await runtimeOverrideTask;
                ApplyOverview(await overviewTask);
            },
            cancellationToken);
    }

    private void ScheduleRuntimeOverrideExpiryRefresh()
    {
        _runtimeOverrideExpiryRefresh?.Cancel();
        _runtimeOverrideExpiryRefresh?.Dispose();
        _runtimeOverrideExpiryRefresh = null;
        var status = _lastRuntimeOverride.Value;
        var expiry = new[] { status?.Active?.ExpiresAt, status?.Pending?.ExpiresAt }
            .Where(value => value is not null)
            .Select(value => value!.Value)
            .DefaultIfEmpty()
            .Min();
        if (expiry == default)
        {
            return;
        }
        var delay = expiry - DateTimeOffset.UtcNow + TimeSpan.FromMilliseconds(100);
        if (delay <= TimeSpan.Zero)
        {
            delay = TimeSpan.FromMilliseconds(100);
        }
        if (delay > TimeSpan.FromHours(1) + TimeSpan.FromMinutes(1))
        {
            return;
        }
        var refresh = new CancellationTokenSource();
        _runtimeOverrideExpiryRefresh = refresh;
        _ = RefreshRuntimeOverrideAfterAsync(delay, refresh);
    }

    private async Task RefreshRuntimeOverrideAfterAsync(
        TimeSpan delay,
        CancellationTokenSource refresh)
    {
        try
        {
            await Task.Delay(delay, refresh.Token);
        }
        catch (OperationCanceledException) when (refresh.IsCancellationRequested)
        {
            return;
        }
        if (!ReferenceEquals(_runtimeOverrideExpiryRefresh, refresh))
        {
            return;
        }
        Dispatcher.UIThread.Post(() =>
            _ = RefreshRuntimeCommand.ExecuteAsync(null));
    }
}
