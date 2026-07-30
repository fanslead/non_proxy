using Avalonia;
using Avalonia.Headless;

[assembly: AvaloniaTestApplication(typeof(NonProxy.Desktop.Tests.TestAppBuilder))]

namespace NonProxy.Desktop.Tests;

public static class TestAppBuilder
{
    public static AppBuilder BuildAvaloniaApp()
    {
        return AppBuilder
            .Configure<TestApplication>()
            .UseHeadless(new AvaloniaHeadlessPlatformOptions());
    }

    public sealed class TestApplication : Application
    {
    }
}
