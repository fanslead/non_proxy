namespace NonProxy.Desktop.Mac;

internal static class MacRuntimePaths
{
    private const string AppGroupIdentifier = "group.com.nonproxy.shared";

    internal static string ResolveDefaultStateDirectory()
    {
        var userProfile = Environment.GetFolderPath(
            Environment.SpecialFolder.UserProfile);
        if (string.IsNullOrWhiteSpace(userProfile)
            || !Path.IsPathFullyQualified(userProfile))
        {
            throw new InvalidOperationException("无法定位 macOS 用户目录。");
        }

        return Path.Combine(
            userProfile,
            "Library",
            "Group Containers",
            AppGroupIdentifier,
            "Library",
            "Application Support",
            "NonProxy");
    }

    internal static string ResolveAdapterStateDirectory()
    {
        return Path.Combine(
            ResolveDefaultStateDirectory(),
            "adapter-host");
    }
}
