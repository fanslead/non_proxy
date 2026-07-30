using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Transport;

namespace NonProxy.Desktop.Tests;

public sealed class ControlTransportTests
{
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
