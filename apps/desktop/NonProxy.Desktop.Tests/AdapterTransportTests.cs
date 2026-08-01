using NonProxy.Desktop.Core.Services.Adapters.Transport;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Windows;

namespace NonProxy.Desktop.Tests;

public sealed class AdapterTransportTests
{
    private const string WindowsUserSid = "S-1-5-21-1000-2000-3000-1001";

    [Fact]
    public void AdapterEndpointUsesItsOwnPrivateStateDirectory()
    {
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-adapter-transport-test");
        var endpoint = LocalAdapterEndpoint.FromStateDirectory(stateDirectory);

        Assert.Equal(
            Path.Combine(stateDirectory, "adapter-host.sock"),
            endpoint.SocketPath);
        Assert.Equal(
            Path.Combine(stateDirectory, "adapter.capability"),
            endpoint.CapabilityPath);
        Assert.True(endpoint.IsConfigured);
    }

    [Fact]
    public void AdapterEndpointRejectsSocketOutsideItsStateDirectory()
    {
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-adapter-transport-test");

        var exception = Assert.Throws<ArgumentException>(() =>
            LocalAdapterEndpoint.FromStateDirectory(
                stateDirectory,
                Path.Combine(Path.GetTempPath(), "escaped.sock")));

        Assert.Equal("socketPath", exception.ParamName);
    }

    [Fact]
    public void WindowsEndpointUsesPerUserStateAndPrivatePipeNamespace()
    {
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-windows-adapter-test");
        var endpoint = LocalAdapterEndpoint.FromWindowsStateDirectory(
            stateDirectory,
            LocalAdapterEndpoint.WindowsPipeForUserSid(WindowsUserSid));

        Assert.Null(endpoint.SocketPath);
        Assert.Equal(
            @"\\.\pipe\NonProxy.Adapter.S-1-5-21-1000-2000-3000-1001",
            endpoint.NamedPipePath);
        Assert.Equal(
            Path.Combine(stateDirectory, "adapter.capability"),
            endpoint.CapabilityPath);
        Assert.True(endpoint.IsConfigured);
    }

    [Fact]
    public void WindowsProductionEndpointIgnoresAdapterEnvironmentOverrides()
    {
        var localApplicationData = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-windows-adapter-environment-test");
        var previousStateDirectory = Environment.GetEnvironmentVariable(
            LocalAdapterEndpoint.StateDirectoryEnvironment);
        var previousPipePath = Environment.GetEnvironmentVariable(
            "NONPROXY_WINDOWS_ADAPTER_PIPE");

        try
        {
            Environment.SetEnvironmentVariable(
                LocalAdapterEndpoint.StateDirectoryEnvironment,
                Path.Combine(Path.GetTempPath(), "attacker-state"));
            Environment.SetEnvironmentVariable(
                "NONPROXY_WINDOWS_ADAPTER_PIPE",
                @"\\.\pipe\NonProxy.Adapter.attacker");

            var endpoint = WindowsAdapterEndpointFactory.CreateForUser(
                localApplicationData,
                WindowsUserSid);

            Assert.Null(endpoint.SocketPath);
            Assert.Equal(
                @"\\.\pipe\NonProxy.Adapter.S-1-5-21-1000-2000-3000-1001",
                endpoint.NamedPipePath);
            Assert.Equal(
                Path.Combine(
                    localApplicationData,
                    "NonProxy",
                    "adapter-host",
                    "adapter.capability"),
                endpoint.CapabilityPath);
            Assert.True(endpoint.IsConfigured);
        }
        finally
        {
            Environment.SetEnvironmentVariable(
                LocalAdapterEndpoint.StateDirectoryEnvironment,
                previousStateDirectory);
            Environment.SetEnvironmentVariable(
                "NONPROXY_WINDOWS_ADAPTER_PIPE",
                previousPipePath);
        }
    }

    [Fact]
    public void WindowsEndpointValidatesNamespaceAndMaximumLength()
    {
        const string prefix = @"\\.\pipe\NonProxy.";
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-windows-adapter-test");
        var maximumPipePath = prefix + new string('A', 160 - prefix.Length);

        var endpoint = LocalAdapterEndpoint.FromWindowsStateDirectory(
            stateDirectory,
            maximumPipePath);
        var oversized = Assert.Throws<ArgumentException>(() =>
            LocalAdapterEndpoint.FromWindowsStateDirectory(
                stateDirectory,
                maximumPipePath + "B"));
        var outsideNamespace = Assert.Throws<ArgumentException>(() =>
            LocalAdapterEndpoint.FromWindowsStateDirectory(
                stateDirectory,
                @"\\.\pipe\Other.Adapter"));

        Assert.Equal(maximumPipePath, endpoint.NamedPipePath);
        Assert.Equal("pipePath", oversized.ParamName);
        Assert.Equal("pipePath", outsideNamespace.ParamName);
    }

    [Fact]
    public void WindowsPipeBindsCanonicalUserSidAndRejectsServices()
    {
        Assert.Equal(
            @"\\.\pipe\NonProxy.Adapter.S-1-5-21-1000-2000-3000-1001",
            LocalAdapterEndpoint.WindowsPipeForUserSid(WindowsUserSid));
        foreach (var invalid in new[]
        {
            "S-1-5-18",
            "S-1-5-19",
            "S-1-5-20",
            "S-1-5-80-1234",
            "s-1-5-21-1001",
            "S-01-5-21-1001",
            "S-1-05-21-1001",
            "S-1-5",
            "S-1-281474976710656-1",
            "S-1-5-4294967296",
        })
        {
            Assert.Throws<ArgumentException>(() =>
                LocalAdapterEndpoint.WindowsPipeForUserSid(invalid));
        }
    }

    [Fact]
    public void WindowsFactoryCreatesAChannelForConfiguredProductPipe()
    {
        var stateDirectory = Path.Combine(
            Path.GetTempPath(),
            "nonproxy-windows-adapter-channel-test");
        var endpoint = LocalAdapterEndpoint.FromWindowsStateDirectory(
            stateDirectory,
            LocalAdapterEndpoint.WindowsPipeForUserSid(WindowsUserSid));
        var factory = new WindowsNamedPipeAdapterChannelFactory(endpoint);

        using var channel = factory.CreateChannel();

        Assert.NotNull(channel);
    }

    [Fact]
    public void WindowsFactoryRejectsManuallyConstructedForeignPipe()
    {
        var endpoint = new LocalAdapterEndpoint(
            null,
            Path.Combine(Path.GetTempPath(), "adapter.capability"),
            @"\\.\pipe\Other.Adapter");
        var factory = new WindowsNamedPipeAdapterChannelFactory(endpoint);

        var exception = Assert.Throws<ControlServiceException>(
            factory.CreateChannel);

        Assert.Equal("NP_ADAPTER_UNAVAILABLE", exception.Code);
    }
}
