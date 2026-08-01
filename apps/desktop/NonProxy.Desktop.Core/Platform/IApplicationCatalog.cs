namespace NonProxy.Desktop.Core.Platform;

public sealed record ApplicationCatalogEntry(
    string DisplayName,
    string StableIdentity,
    string? SignerIdentity,
    string? BundleIdentifier,
    bool IsRunning,
    string? BundlePath = null,
    bool IncludeHelpers = true)
{
    public string StateLabel => IsRunning ? "正在运行" : "已安装";

    public string IdentityAssuranceLabel => string.IsNullOrWhiteSpace(SignerIdentity)
        ? "由系统应用身份识别"
        : "已校验开发者签名";
}

public sealed record ApplicationCatalogSnapshot(
    IReadOnlyList<ApplicationCatalogEntry> Applications,
    bool IsAvailable,
    bool CanChooseApplication,
    string Message)
{
    public static ApplicationCatalogSnapshot Unavailable(string message)
    {
        return new(
            Array.Empty<ApplicationCatalogEntry>(),
            false,
            false,
            message);
    }
}

public sealed record ApplicationSelectionResult(
    bool Succeeded,
    ApplicationCatalogEntry? Application,
    string Message);

public interface IApplicationCatalog
{
    Task<ApplicationCatalogSnapshot> ListAsync(
        CancellationToken cancellationToken);

    Task<ApplicationSelectionResult> ChooseAsync(
        CancellationToken cancellationToken);
}
