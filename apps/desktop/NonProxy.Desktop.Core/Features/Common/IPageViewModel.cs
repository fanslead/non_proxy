using CommunityToolkit.Mvvm.Input;

namespace NonProxy.Desktop.Core.Features.Common;

public interface IPageViewModel
{
    string Title { get; }

    IAsyncRelayCommand RefreshCommand { get; }
}
