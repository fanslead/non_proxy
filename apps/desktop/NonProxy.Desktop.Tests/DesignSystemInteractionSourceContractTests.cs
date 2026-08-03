namespace NonProxy.Desktop.Tests;

public sealed class DesignSystemInteractionSourceContractTests
{
    [Fact]
    public void InputThemesRetainFluentBehaviorAndExposeInteractionFeedback()
    {
        var controls = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(),
            "apps",
            "desktop",
            "NonProxy.Desktop.Core",
            "DesignSystem",
            "Controls.axaml"));

        Assert.Equal(
            2,
            CountOccurrences(
                controls,
                "BasedOn=\"{StaticResource {x:Type Button}}\""));
        Assert.Contains(
            "BasedOn=\"{StaticResource {x:Type TextBox}}\"",
            controls,
            StringComparison.Ordinal);
        Assert.Contains("^:pointerover", controls, StringComparison.Ordinal);
        Assert.Contains("^:pressed", controls, StringComparison.Ordinal);
        Assert.Contains("^:focus-visible", controls, StringComparison.Ordinal);
        Assert.Contains("^:disabled", controls, StringComparison.Ordinal);
        Assert.Contains("BrushTransition", controls, StringComparison.Ordinal);
        Assert.Contains("DoubleTransition", controls, StringComparison.Ordinal);
    }

    private static int CountOccurrences(string source, string value)
    {
        return source.Split(value, StringSplitOptions.None).Length - 1;
    }

    private static string FindRepositoryRoot()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "Cargo.toml"))
                && Directory.Exists(Path.Combine(
                    directory.FullName,
                    "apps",
                    "desktop")))
            {
                return directory.FullName;
            }
            directory = directory.Parent;
        }
        throw new DirectoryNotFoundException("无法定位 NonProxy Monorepo 根目录。");
    }
}
