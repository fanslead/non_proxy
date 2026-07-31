using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Core.Features.Applications;

public sealed record ApplicationCatalogItem(
    ApplicationCatalogEntry Application,
    bool IsConfigured)
{
    public string DisplayName => Application.DisplayName;

    public string StateLabel => Application.StateLabel;

    public string IdentityAssuranceLabel => Application.IdentityAssuranceLabel;

    public string ActionLabel => IsConfigured ? "已直连" : "设为直连";
}
