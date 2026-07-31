using NonProxy.Adapter.V1;

namespace NonProxy.Desktop.Core.Features.Adapters;

public sealed record AdapterClientOption(
    string DisplayName,
    AdapterClient Client,
    string DefaultId,
    string ManagedFileName,
    string ExecutableHint,
    string ConfigurationHint,
    string DirectTargetHint)
{
    public override string ToString()
    {
        return DisplayName;
    }
}
