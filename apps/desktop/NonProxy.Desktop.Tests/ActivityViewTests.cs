using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using NonProxy.Desktop.Core.Features.Activity;
using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Tests;

public sealed class ActivityViewTests
{
    [AvaloniaFact]
    public async Task SignedActivityRendersQuickActionAndExplicitConfirmation()
    {
        var viewModel = new ActivityViewModel(
            new ActivityViewModelTests.FixedActivityService(
                ActivityViewModelTests.Activity(
                    1,
                    "财务客户端",
                    "com.example.finance",
                    signerId: "TEAM-FINANCE")),
            new ActivityViewModelTests.RecordingPolicyService(),
            new ActivityViewModelTests.TestPlatformInformation(
                PlatformKind.MacOS));
        var view = new ActivityView { DataContext = viewModel };
        var window = new Window { Width = 1_120, Height = 820, Content = view };

        try
        {
            window.Show();
            await viewModel.RefreshCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            var list = Assert.IsType<ItemsControl>(
                view.FindControl<ItemsControl>("ActivityItemsList"));
            var item = Assert.Single(viewModel.Items);
            var row = Assert.IsAssignableFrom<Avalonia.Controls.Control>(
                list.ItemTemplate?.Build(item));
            var prepare = Assert.Single(
                row.GetLogicalDescendants().OfType<Button>(),
                button => Equals(button.Content, "让此应用始终直连"));
            Assert.Equal(
                "准备从活动记录创建应用直连规则",
                AutomationProperties.GetName(prepare));
            Assert.True(item.CanPrepareDirect);
            Assert.Single(list.Items.Cast<object>());

            viewModel.RequestDirectCommand.Execute(item);
            Dispatcher.UIThread.RunJobs();

            Assert.True(view.FindControl<Border>("ActivityDirectConfirmation")?.IsVisible);
            var confirm = Assert.IsType<Button>(
                view.FindControl<Button>("ConfirmActivityDirectButton"));
            Assert.Equal("确认创建活动直连规则", AutomationProperties.GetName(confirm));
            Assert.Contains(
                view.GetLogicalDescendants().OfType<TextBlock>(),
                block => block.Text?.Contains(
                    "TEAM-FINANCE",
                    StringComparison.Ordinal) == true);
        }
        finally
        {
            window.Close();
        }
    }
}
