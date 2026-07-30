namespace NonProxy.Desktop.Core.Platform;

public interface IPlatformInformation
{
    PlatformKind Platform { get; }

    string DisplayName { get; }
}
