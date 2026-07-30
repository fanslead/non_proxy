using Avalonia.Controls;
using NonProxy.Desktop.Core.Features.Shell;

namespace NonProxy.Desktop.Core.Views;

public partial class MainWindow : Window
{
    private bool _hasInitialized;

    public MainWindow()
    {
        InitializeComponent();
        Opened += OnOpened;
    }

    public MainWindow(MainWindowViewModel viewModel)
        : this()
    {
        DataContext = viewModel;
    }

    private async void OnOpened(object? sender, EventArgs eventArgs)
    {
        if (_hasInitialized || DataContext is not MainWindowViewModel viewModel)
        {
            return;
        }

        _hasInitialized = true;
        await viewModel.InitializeCommand.ExecuteAsync(null);
    }
}
