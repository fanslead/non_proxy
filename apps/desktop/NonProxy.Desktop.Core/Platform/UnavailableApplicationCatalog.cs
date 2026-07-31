namespace NonProxy.Desktop.Core.Platform;

internal sealed class UnavailableApplicationCatalog : IApplicationCatalog
{
    private const string UnavailableMessage =
        "当前平台尚未连接应用选择器；已有应用规则仍可正常查看和删除。";

    public Task<ApplicationCatalogSnapshot> ListAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(
            ApplicationCatalogSnapshot.Unavailable(UnavailableMessage));
    }

    public Task<ApplicationSelectionResult> ChooseAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(new ApplicationSelectionResult(
            false,
            null,
            UnavailableMessage));
    }
}
