using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Common;

public abstract partial class LoadableViewModel : ObservableObject, IPageViewModel
{
    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    protected LoadableViewModel(string title)
    {
        Title = title;
        RefreshCommand = new AsyncRelayCommand(
            RefreshAsync,
            AsyncRelayCommandOptions.None);
    }

    public string Title { get; }

    public IAsyncRelayCommand RefreshCommand { get; }

    public bool HasError => !string.IsNullOrWhiteSpace(ErrorMessage);

    partial void OnErrorMessageChanged(string? value)
    {
        OnPropertyChanged(nameof(HasError));
    }

    protected abstract Task LoadCoreAsync(CancellationToken cancellationToken);

    protected void ClearError()
    {
        ErrorMessage = null;
    }

    protected async Task RunOperationAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(operation);

        try
        {
            IsBusy = true;
            ClearError();
            await operation(cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (ControlServiceException exception)
        {
            ErrorMessage = exception.UserMessage;
        }
        catch (Exception)
        {
            ErrorMessage = "操作未完成，请稍后重试；如持续失败，请打开“诊断”查看详细状态。";
        }
        finally
        {
            IsBusy = false;
        }
    }

    private Task RefreshAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(LoadCoreAsync, cancellationToken);
    }
}
