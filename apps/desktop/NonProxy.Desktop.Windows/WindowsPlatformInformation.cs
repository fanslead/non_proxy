using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Windows;

internal sealed class WindowsPlatformInformation : IPlatformInformation
{
    public PlatformKind Platform => PlatformKind.Windows;

    public string DisplayName => "Windows";
}
