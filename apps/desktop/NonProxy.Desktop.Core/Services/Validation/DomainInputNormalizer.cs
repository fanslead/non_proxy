using System.Globalization;

namespace NonProxy.Desktop.Core.Services.Validation;

public static class DomainInputNormalizer
{
    public static bool TryNormalize(
        string? input,
        out string normalized,
        out string error)
    {
        normalized = string.Empty;
        error = string.Empty;

        var candidate = input?.Trim().TrimEnd('.');
        if (string.IsNullOrWhiteSpace(candidate))
        {
            error = "请输入网站域名。";
            return false;
        }

        if (candidate.Contains("://", StringComparison.Ordinal)
            || candidate.Contains('/', StringComparison.Ordinal)
            || candidate.Contains(':', StringComparison.Ordinal))
        {
            error = "这里只填写域名，例如 example.com，不要包含协议、端口或路径。";
            return false;
        }

        try
        {
            var ascii = new IdnMapping().GetAscii(candidate).ToLowerInvariant();
            if (Uri.CheckHostName(ascii) != UriHostNameType.Dns)
            {
                error = "域名格式无效，请检查后重试。";
                return false;
            }

            normalized = ascii;
            return true;
        }
        catch (ArgumentException)
        {
            error = "域名包含无法识别的字符。";
            return false;
        }
    }
}
