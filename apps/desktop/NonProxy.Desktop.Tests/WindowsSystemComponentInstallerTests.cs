using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Windows;

namespace NonProxy.Desktop.Tests;

public sealed class WindowsSystemComponentInstallerTests
{
    [Fact]
    public async Task MapsAuthoritativeBootstrapStateAndSteps()
    {
        var installer = new WindowsSystemComponentInstaller(new Bootstrap
        {
            QueryJson =
                """
                {"success":true,"status":"Installed","message":"ready","errorCode":null,"requiresReboot":false,"steps":[{"id":"gateway","name":"后台服务","installed":true,"status":"Running"},{"id":"adapter-host","name":"客户端适配服务","installed":true,"status":"Ready"}]}
                """,
        });

        var state = await installer.GetStateAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal(SystemComponentStatus.Installed, state.Status);
        Assert.All(
            state.Steps,
            step => Assert.Equal(SystemComponentStepStatus.Ready, step.Status));
    }

    [Fact]
    public async Task KeepsUacCancellationDistinctFromTransactionFailure()
    {
        var installer = new WindowsSystemComponentInstaller(new Bootstrap
        {
            Mutation = new WindowsBootstrapMutationResult(
                false,
                false,
                true,
                1223),
        });

        var result = await installer.InstallAsync(
            TestContext.Current.CancellationToken);

        Assert.False(result.Success);
        Assert.Equal("NP_WINDOWS_ELEVATION_CANCELLED", result.ErrorCode);
        Assert.Contains("未变更", result.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task MissingCompiledPublisherRemainsExplicitlyUnavailable()
    {
        var installer = new WindowsSystemComponentInstaller(new Bootstrap
        {
            QueryException = new WindowsBootstrapException(
                "publisher missing",
                "NP_WINDOWS_PUBLISHER_NOT_CONFIGURED"),
        });

        var state = await installer.GetStateAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal(SystemComponentStatus.Unavailable, state.Status);
        Assert.Equal("NP_WINDOWS_PUBLISHER_NOT_CONFIGURED", state.ErrorCode);
    }

    private sealed class Bootstrap : IWindowsComponentBootstrap
    {
        public string QueryJson { get; init; } =
            """{"success":true,"status":"NotInstalled","message":"absent","errorCode":null,"requiresReboot":false,"steps":[]}""";

        public WindowsBootstrapMutationResult Mutation { get; init; } =
            new(true, false, false, 0);

        public WindowsBootstrapException? QueryException { get; init; }

        public Task<WindowsBootstrapQueryResult> QueryAsync(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (QueryException is not null)
            {
                throw QueryException;
            }
            return Task.FromResult(new WindowsBootstrapQueryResult(0, QueryJson));
        }

        public Task<WindowsBootstrapMutationResult> MutateAsync(
            WindowsBootstrapAction action,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(Mutation);
        }
    }
}
