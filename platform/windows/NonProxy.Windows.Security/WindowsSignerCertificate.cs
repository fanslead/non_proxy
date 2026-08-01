namespace NonProxy.Windows.Security;

public sealed record WindowsSignerCertificate(
    string ThumbprintSha1,
    string CertificateSha256)
{
    public string StableIdentity => $"cert-sha256:{CertificateSha256}";
}
