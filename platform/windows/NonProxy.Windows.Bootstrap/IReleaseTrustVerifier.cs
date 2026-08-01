using System.Runtime.Versioning;
using NonProxy.Windows.Security;

namespace NonProxy.Windows.Bootstrap;

public interface IReleaseTrustVerifier
{
    WindowsSignerCertificate? VerifyAuthenticode(string path);

    void VerifyCatalogMember(string catalogPath, string memberPath);
}

[SupportedOSPlatform("windows10.0.18362.0")]
internal sealed class WindowsReleaseTrustVerifier : IReleaseTrustVerifier
{
    public WindowsSignerCertificate? VerifyAuthenticode(string path) =>
        WindowsAuthenticodeTrust.VerifyFile(path);

    public void VerifyCatalogMember(string catalogPath, string memberPath) =>
        WindowsCatalogTrust.VerifyMember(catalogPath, memberPath);
}
