using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Transport;
using NonProxy.Desktop.Windows;

namespace NonProxy.Desktop.Tests;

public sealed class ControlTransportTests
{
    [Fact]
    public void EndpointDerivesControlFilesFromOneStateDirectory()
    {
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-control-test");

        var endpoint = LocalControlEndpoint.FromStateDirectory(stateDirectory);

        Assert.Equal(
            Path.Combine(stateDirectory, "gatewayd.sock"),
            endpoint.SocketPath);
        Assert.Equal(
            Path.Combine(stateDirectory, "session.capability"),
            endpoint.SessionCapabilityPath);
    }

    [Fact]
    public void WindowsEndpointUsesProgramDataCapabilityAndPrivatePipeNamespace()
    {
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-windows-control-test");

        var endpoint = LocalControlEndpoint.FromWindowsStateDirectory(stateDirectory);

        Assert.Null(endpoint.SocketPath);
        Assert.Equal(
            LocalControlEndpoint.DefaultWindowsControlPipe,
            endpoint.NamedPipePath);
        Assert.Equal(
            Path.Combine(stateDirectory, "session.capability"),
            endpoint.SessionCapabilityPath);
        Assert.True(endpoint.IsConfigured);
    }

    [Fact]
    public void WindowsProductionEndpointIgnoresControlEnvironmentOverrides()
    {
        var commonApplicationData = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-windows-environment-test");
        var previousStateDirectory = Environment.GetEnvironmentVariable(
            LocalControlEndpoint.StateDirectoryEnvironment);
        var previousPipePath = Environment.GetEnvironmentVariable(
            "NONPROXY_WINDOWS_CONTROL_PIPE");

        try
        {
            Environment.SetEnvironmentVariable(
                LocalControlEndpoint.StateDirectoryEnvironment,
                Path.Combine(Path.GetTempPath(), "attacker-state"));
            Environment.SetEnvironmentVariable(
                "NONPROXY_WINDOWS_CONTROL_PIPE",
                @"\\.\pipe\NonProxy.Control.attacker");

            var endpoint = WindowsControlEndpointFactory.CreateForMachine(
                commonApplicationData);

            Assert.Null(endpoint.SocketPath);
            Assert.Equal(
                LocalControlEndpoint.DefaultWindowsControlPipe,
                endpoint.NamedPipePath);
            Assert.Equal(
                Path.Combine(
                    commonApplicationData,
                    "NonProxy",
                    "session.capability"),
                endpoint.SessionCapabilityPath);
            Assert.True(endpoint.IsConfigured);
        }
        finally
        {
            Environment.SetEnvironmentVariable(
                LocalControlEndpoint.StateDirectoryEnvironment,
                previousStateDirectory);
            Environment.SetEnvironmentVariable(
                "NONPROXY_WINDOWS_CONTROL_PIPE",
                previousPipePath);
        }
    }

    [Theory]
    [InlineData(null, "/var/lib/nonproxy/session.capability", null, false)]
    [InlineData("/var/run/nonproxy.sock", "/var/lib/nonproxy/session.capability", null, true)]
    [InlineData(null, "/var/lib/nonproxy/session.capability", @"\\.\pipe\NonProxy.Control.v1", true)]
    [InlineData(
        "/var/run/nonproxy.sock",
        "/var/lib/nonproxy/session.capability",
        @"\\.\pipe\NonProxy.Control.v1",
        false)]
    [InlineData("/var/run/nonproxy.sock", null, null, false)]
    [InlineData(null, " ", @"\\.\pipe\NonProxy.Control.v1", false)]
    public void EndpointRequiresCapabilityAndExactlyOneLocalTransport(
        string? socketPath,
        string? capabilityPath,
        string? pipePath,
        bool expected)
    {
        var endpoint = new LocalControlEndpoint(
            socketPath,
            capabilityPath,
            pipePath);

        Assert.Equal(expected, endpoint.IsConfigured);
    }

    [Fact]
    public void WindowsEndpointAcceptsMaximumLengthPipeAndRejectsLongerValue()
    {
        const string prefix = @"\\.\pipe\NonProxy.";
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-windows-control-test");
        var maximumPipePath = prefix + new string('A', 160 - prefix.Length);
        var oversizedPipePath = maximumPipePath + "B";

        var endpoint = LocalControlEndpoint.FromWindowsStateDirectory(
            stateDirectory,
            maximumPipePath);
        var exception = Assert.Throws<ArgumentException>(
            () => LocalControlEndpoint.FromWindowsStateDirectory(
                stateDirectory,
                oversizedPipePath));

        Assert.Equal(maximumPipePath, endpoint.NamedPipePath);
        Assert.Equal("pipePath", exception.ParamName);
    }

    [Theory]
    [InlineData(@"\\.\pipe\NonProxy.")]
    [InlineData(@"\\.\pipe\Other.Control")]
    [InlineData(@"\\.\pipe\nonproxy.Control")]
    [InlineData(@"\\server\pipe\NonProxy.Control")]
    [InlineData(@"\\.\pipe\NonProxy.Invalid\Child")]
    [InlineData(@"\\.\pipe\NonProxy.Invalid:Child")]
    [InlineData(@"\\.\pipe\NonProxy.控制")]
    public void WindowsEndpointRejectsPipeOutsideProductNamespace(string pipePath)
    {
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-windows-control-test");

        Assert.Throws<ArgumentException>(
            () => LocalControlEndpoint.FromWindowsStateDirectory(
                stateDirectory,
                pipePath));
    }

    [Fact]
    public void WindowsEndpointRejectsRelativeStateDirectory()
    {
        var exception = Assert.Throws<ArgumentException>(
            () => LocalControlEndpoint.FromWindowsStateDirectory(
                "relative-state",
                LocalControlEndpoint.DefaultWindowsControlPipe));

        Assert.Equal("stateDirectory", exception.ParamName);
    }

    [Fact]
    public async Task CapabilityProviderReadsExactlyThirtyTwoBytes()
    {
        var directory = Directory.CreateTempSubdirectory("nonproxy-token-");
        try
        {
            var capabilityPath = Path.Combine(directory.FullName, "session.capability");
            var expected = Enumerable.Range(0, FileSessionCapabilityProvider.TokenLength)
                .Select(value => (byte)value)
                .ToArray();
            await File.WriteAllBytesAsync(
                capabilityPath,
                expected,
                TestContext.Current.CancellationToken);
            var provider = new FileSessionCapabilityProvider(
                new LocalControlEndpoint(
                    Path.Combine(directory.FullName, "gatewayd.sock"),
                    capabilityPath));

            var actual = await provider.ReadAsync(
                TestContext.Current.CancellationToken);

            Assert.Equal(expected, actual);
        }
        finally
        {
            directory.Delete(recursive: true);
        }
    }

    [Fact]
    public async Task CapabilityProviderRejectsWrongLengthWithoutReturningBytes()
    {
        var directory = Directory.CreateTempSubdirectory("nonproxy-token-");
        try
        {
            var capabilityPath = Path.Combine(directory.FullName, "session.capability");
            await File.WriteAllBytesAsync(
                capabilityPath,
                [1, 2, 3],
                TestContext.Current.CancellationToken);
            var provider = new FileSessionCapabilityProvider(
                new LocalControlEndpoint(
                    Path.Combine(directory.FullName, "gatewayd.sock"),
                    capabilityPath));

            var exception = await Assert.ThrowsAsync<ControlServiceException>(
                () => provider.ReadAsync(TestContext.Current.CancellationToken));

            Assert.Equal("NP_SESSION_TOKEN_INVALID", exception.Code);
        }
        finally
        {
            directory.Delete(recursive: true);
        }
    }
}
