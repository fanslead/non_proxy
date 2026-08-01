namespace NonProxy.Windows.Bootstrap;

public enum BootstrapAction
{
    Query,
    Install,
    Repair,
    Uninstall,
}

public sealed record BootstrapArguments(
    BootstrapAction Action,
    string PackageRoot)
{
    public static BootstrapArguments Parse(IReadOnlyList<string> args)
    {
        if (args.Count != 3 || args[1] != "--package-root")
        {
            throw new ArgumentException(
                "参数必须是 <query|install|repair|uninstall> --package-root <path>。",
                nameof(args));
        }
        var action = args[0] switch
        {
            "query" => BootstrapAction.Query,
            "install" => BootstrapAction.Install,
            "repair" => BootstrapAction.Repair,
            "uninstall" => BootstrapAction.Uninstall,
            _ => throw new ArgumentException("Bootstrap action 无效。", nameof(args)),
        };
        ArgumentException.ThrowIfNullOrWhiteSpace(args[2]);
        return new BootstrapArguments(action, Path.GetFullPath(args[2]));
    }
}
