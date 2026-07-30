using NonProxy.Desktop.Core.Services.Validation;

namespace NonProxy.Desktop.Tests;

public sealed class DomainInputNormalizerTests
{
    [Theory]
    [InlineData(" Example.COM. ", "example.com")]
    [InlineData("例子.测试", "xn--fsqu00a.xn--0zwm56d")]
    public void ValidDomainIsConvertedToStableAscii(
        string input,
        string expected)
    {
        var success = DomainInputNormalizer.TryNormalize(
            input,
            out var normalized,
            out var error);

        Assert.True(success);
        Assert.Equal(expected, normalized);
        Assert.Empty(error);
    }

    [Theory]
    [InlineData("")]
    [InlineData("https://example.com/path")]
    [InlineData("example.com:443")]
    [InlineData("not a domain")]
    public void NonDomainInputIsRejected(string input)
    {
        var success = DomainInputNormalizer.TryNormalize(
            input,
            out _,
            out var error);

        Assert.False(success);
        Assert.NotEmpty(error);
    }
}
