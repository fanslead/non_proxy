using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using NonProxy.Adapter.V1;
using NonProxy.Common.V1;
using NonProxy.Desktop.Core.Features.Adapters;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;

namespace NonProxy.Desktop.Tests;

public sealed class AdaptersViewTests
{
    [AvaloniaFact]
    public Task MacCompositionRendersAdapterWorkflow()
    {
        return AssertAdapterWorkflowAsync(PlatformKind.MacOS, "macOS");
    }

    [AvaloniaFact]
    public Task WindowsCompositionRendersAdapterWorkflow()
    {
        return AssertAdapterWorkflowAsync(PlatformKind.Windows, "Windows");
    }

    private static async Task AssertAdapterWorkflowAsync(
        PlatformKind platform,
        string displayName)
    {
        using var services = TestPlatformServices.Create(
            platform,
            displayName,
            registrations => registrations.AddSingleton<IAdapterManagementService>(
                new VisibleAdapterManagementService()));
        var viewModel = services.GetRequiredService<AdaptersViewModel>();
        var view = new AdaptersView { DataContext = viewModel };
        var window = new Window
        {
            Width = 1_200,
            Height = 900,
            Content = view,
        };

        try
        {
            window.Show();
            await viewModel.RefreshCommand.ExecuteAsync(null);
            Dispatcher.UIThread.RunJobs();

            Assert.NotNull(view.FindControl<AdapterEvidenceView>("AdapterEvidenceRail"));
            Assert.NotNull(view.FindControl<ComboBox>("AdapterClientTypeSelector"));
            Assert.NotNull(view.FindControl<TextBox>("AdapterExecutablePath"));
            var chooseExecutable = Assert.IsType<Button>(
                view.FindControl<Button>("ChooseAdapterExecutableButton"));
            var chooseConfiguration = Assert.IsType<Button>(
                view.FindControl<Button>("ChooseAdapterConfigurationButton"));
            Assert.Equal(
                "选择代理客户端或可执行文件",
                AutomationProperties.GetName(chooseExecutable));
            Assert.Equal(
                "选择代理客户端当前主配置",
                AutomationProperties.GetName(chooseConfiguration));
            Assert.True(chooseExecutable.IsEnabled);
            Assert.True(chooseConfiguration.IsEnabled);
            var register = Assert.IsType<Button>(
                view.FindControl<Button>("AdapterRegisterButton"));
            Assert.Equal(
                "校验并登记代理客户端",
                AutomationProperties.GetName(register));

            var list = view.FindControl<ItemsControl>("AdapterInstallationsList");
            var row = list?.ItemTemplate?.Build(viewModel.Items.Single());
            Assert.NotNull(row);
            var actions = row
                .GetLogicalDescendants()
                .OfType<Button>()
                .ToDictionary(button => button.Content as string ?? string.Empty);
            Assert.Equal(
                "校验并同步代理客户端规则",
                AutomationProperties.GetName(actions["校验并同步"]));
            Assert.Equal(
                "移除代理客户端登记",
                AutomationProperties.GetName(actions["移除登记"]));
        }
        finally
        {
            window.Close();
        }
    }

    private sealed class VisibleAdapterManagementService : IAdapterManagementService
    {
        private static readonly AdapterInstallationItem Installation = new(
            "surge-primary",
            AdapterClient.Surge,
            "Surge",
            "5.11.0",
            "/Applications/Surge.app/Contents/Applications/surge-cli",
            "/tmp/nonproxy.list",
            "/tmp/current.conf",
            null,
            AdapterState.Ready);

        public Task<AdapterCatalog> ListAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new AdapterCatalog([Installation], DateTimeOffset.UtcNow));
        }

        public Task<AdapterMutationResult> RegisterAsync(
            AdapterRegistrationDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new AdapterMutationResult(
                true,
                "NP_ADAPTER_REGISTERED",
                "客户端已登记。",
                Installation));
        }

        public Task<AdapterMutationResult> RemoveAsync(
            string adapterId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new AdapterMutationResult(
                true,
                "NP_ADAPTER_REMOVED",
                "客户端登记已移除。"));
        }

        public Task<AdapterSyncResult> SyncAsync(
            string adapterId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new AdapterSyncResult(
                true,
                "NP_ADAPTER_CONFIGURATION_VERIFIED",
                "配置已载入，等待路径证据。",
                12,
                3,
                ClientValidated: true,
                Reloaded: true,
                ConfigurationVerified: true,
                PathVerified: false,
                EvidenceLevel.Configuration,
                []));
        }
    }
}
