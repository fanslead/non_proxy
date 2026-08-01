namespace NonProxy.Windows.Bootstrap;

public sealed class ReleaseManifest
{
    public int SchemaVersion { get; init; }

    public string? Product { get; init; }

    public string? Version { get; init; }

    public string? Architecture { get; init; }

    public int MinimumWindowsBuild { get; init; }

    public string? PublisherCertificateSha256 { get; init; }

    public string? PublisherThumbprintHint { get; init; }

    public string? SignedUtc { get; init; }

    public ReleaseManifestFile[]? Files { get; init; }
}

public sealed class ReleaseManifestFile
{
    public string? Path { get; init; }

    public long Size { get; init; }

    public string? Sha256 { get; init; }
}

public sealed record ValidatedReleasePackage(
    string PackageRoot,
    string Version,
    string Architecture,
    string ManifestSha256,
    string PublisherThumbprintSha1,
    string PublisherCertificateSha256);
