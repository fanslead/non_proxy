using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Mac;

internal sealed class MacPlatformInformation : IPlatformInformation
{
    public PlatformKind Platform => PlatformKind.MacOS;

    public string DisplayName => "macOS";
}
