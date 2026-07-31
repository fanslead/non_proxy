using NonProxy.Adapter.V1;
using NonProxy.Common.V1;
using NonProxy.Desktop.Core.Features.Adapters;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Adapters;

namespace NonProxy.Desktop.Tests;

public sealed class AdaptersViewModelTests
{
    [Fact]
    public async Task RegistrationSuggestsDedicatedSidecarBesideMainConfiguration()
    {
        var service = new RecordingAdapterManagementService();
        var viewModel = new AdaptersViewModel(service, NoSelectionPicker.Instance)
        {
            SelectedClient = AdaptersViewModel.ClientOptions.Single(option =>
                option.Client == AdapterClient.Mihomo),
            ExecutablePath = "/opt/nonproxy-test/mihomo",
            MainConfigurationPath = "/opt/nonproxy-test/config.yaml",
        };

        await viewModel.RegisterCommand.ExecuteAsync(null);

        var draft = Assert.IsType<AdapterRegistrationDraft>(service.RegisteredDraft);
        Assert.Equal("/opt/nonproxy-test/nonproxy.yaml", draft.ManagedRulesPath);
        Assert.Equal(AdapterClient.Mihomo, draft.Client);
        Assert.Null(viewModel.ValidationMessage);
    }

    [Fact]
    public async Task RegistrationRejectsMainConfigurationAsManagedRulesFile()
    {
        var service = new RecordingAdapterManagementService();
        var viewModel = new AdaptersViewModel(service, NoSelectionPicker.Instance)
        {
            ExecutablePath = "/opt/nonproxy-test/surge-cli",
            MainConfigurationPath = "/opt/nonproxy-test/current.conf",
            ManagedRulesPath = "/opt/nonproxy-test/generated/../current.conf",
        };

        await viewModel.RegisterCommand.ExecuteAsync(null);

        Assert.Null(service.RegisteredDraft);
        Assert.Contains(
            "不能覆盖主配置",
            viewModel.ValidationMessage,
            StringComparison.Ordinal);
    }

    [Fact]
    public async Task SyncPresentsConfigurationAndPathEvidenceSeparately()
    {
        var service = new RecordingAdapterManagementService();
        var viewModel = new AdaptersViewModel(service, NoSelectionPicker.Instance);
        await viewModel.RefreshCommand.ExecuteAsync(null);

        await viewModel.SyncCommand.ExecuteAsync(viewModel.Items.Single());

        Assert.Equal("已校验", viewModel.ClientValidationLabel);
        Assert.Equal("已载入", viewModel.ConfigurationVerificationLabel);
        Assert.Equal("尚未证明绕过 VPN", viewModel.PathVerificationLabel);
        Assert.Equal("活动快照：v27", viewModel.LastSnapshotLabel);
        Assert.Contains("尚未证明", viewModel.LastEvidenceMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task RunningMutationDisablesEveryOtherMutationCommand()
    {
        var service = new RecordingAdapterManagementService
        {
            RegistrationGate = new TaskCompletionSource(
                TaskCreationOptions.RunContinuationsAsynchronously),
        };
        var viewModel = new AdaptersViewModel(service, NoSelectionPicker.Instance)
        {
            ExecutablePath = "/opt/nonproxy-test/surge-cli",
            MainConfigurationPath = "/opt/nonproxy-test/current.conf",
            ManagedRulesPath = "/opt/nonproxy-test/nonproxy.list",
        };
        await viewModel.RefreshCommand.ExecuteAsync(null);

        var operation = viewModel.RegisterCommand.ExecuteAsync(null);
        await service.RegistrationStarted.Task.WaitAsync(
            TimeSpan.FromSeconds(2),
            TestContext.Current.CancellationToken);

        var installation = viewModel.Items.Single();
        Assert.False(viewModel.ChooseExecutableCommand.CanExecute(null));
        Assert.False(viewModel.ChooseConfigurationCommand.CanExecute(null));
        Assert.False(viewModel.RegisterCommand.CanExecute(null));
        Assert.False(viewModel.SyncCommand.CanExecute(installation));
        Assert.False(viewModel.RemoveCommand.CanExecute(installation));

        service.RegistrationGate.SetResult();
        await operation;
        Assert.True(viewModel.SyncCommand.CanExecute(installation));
    }

    [Fact]
    public async Task NativeSelectionStillProducesUntrustedRegistrationCandidates()
    {
        var picker = new RecordingAdapterFilePicker
        {
            ExecutableSelection = AdapterFileSelection.Selected(
                "/Applications/Surge.app"),
            ConfigurationSelection = AdapterFileSelection.Selected(
                "/opt/nonproxy-test/current.conf"),
        };
        var viewModel = new AdaptersViewModel(
            new RecordingAdapterManagementService(),
            picker);

        await viewModel.ChooseExecutableCommand.ExecuteAsync(null);
        await viewModel.ChooseConfigurationCommand.ExecuteAsync(null);

        Assert.Equal(
            "/Applications/Surge.app/Contents/Applications/surge-cli",
            viewModel.ExecutablePath);
        Assert.Equal(
            "/opt/nonproxy-test/current.conf",
            viewModel.MainConfigurationPath);
        Assert.Equal(
            "/opt/nonproxy-test/nonproxy.list",
            viewModel.ManagedRulesPath);
        Assert.Contains("仍会确认", viewModel.OperationMessage, StringComparison.Ordinal);
    }

    private sealed class NoSelectionPicker : IAdapterFilePicker
    {
        public static NoSelectionPicker Instance { get; } = new();

        public Task<AdapterFileSelection> PickExecutableAsync(
            AdapterClient client,
            string clientName,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(AdapterFileSelection.Cancelled);
        }

        public Task<AdapterFileSelection> PickConfigurationAsync(
            AdapterClient client,
            string clientName,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(AdapterFileSelection.Cancelled);
        }
    }

    private sealed class RecordingAdapterFilePicker : IAdapterFilePicker
    {
        public AdapterFileSelection ExecutableSelection { get; init; } =
            AdapterFileSelection.Cancelled;

        public AdapterFileSelection ConfigurationSelection { get; init; } =
            AdapterFileSelection.Cancelled;

        public Task<AdapterFileSelection> PickExecutableAsync(
            AdapterClient client,
            string clientName,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(ExecutableSelection);
        }

        public Task<AdapterFileSelection> PickConfigurationAsync(
            AdapterClient client,
            string clientName,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(ConfigurationSelection);
        }
    }

    private sealed class RecordingAdapterManagementService : IAdapterManagementService
    {
        private static readonly AdapterInstallationItem Installation = new(
            "mihomo-primary",
            AdapterClient.Mihomo,
            "Mihomo",
            "1.19.0",
            "/opt/nonproxy-test/mihomo",
            "/opt/nonproxy-test/nonproxy.yaml",
            "/opt/nonproxy-test/config.yaml",
            null,
            AdapterState.Ready);

        public AdapterRegistrationDraft? RegisteredDraft { get; private set; }

        public TaskCompletionSource? RegistrationGate { get; init; }

        public TaskCompletionSource RegistrationStarted { get; } = new(
            TaskCreationOptions.RunContinuationsAsynchronously);

        public Task<AdapterCatalog> ListAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new AdapterCatalog([Installation], DateTimeOffset.UtcNow));
        }

        public async Task<AdapterMutationResult> RegisterAsync(
            AdapterRegistrationDraft draft,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            RegisteredDraft = draft;
            RegistrationStarted.TrySetResult();
            if (RegistrationGate is not null)
            {
                await RegistrationGate.Task.WaitAsync(cancellationToken);
            }
            return new AdapterMutationResult(
                true,
                "NP_ADAPTER_REGISTERED",
                "客户端已登记。",
                Installation);
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
                "配置已载入，但尚未证明真实流量绕过 VPN。",
                27,
                4,
                ClientValidated: true,
                Reloaded: true,
                ConfigurationVerified: true,
                PathVerified: false,
                EvidenceLevel.Configuration,
                []));
        }
    }
}
