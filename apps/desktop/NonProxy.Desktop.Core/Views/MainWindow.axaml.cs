using Avalonia.Controls;
using NonProxy.Desktop.Core.Features.Shell;

namespace NonProxy.Desktop.Core.Views;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
    }

    public MainWindow(MainWindowViewModel viewModel)
        : this()
    {
        DataContext = viewModel;
    }
}
