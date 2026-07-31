using NonProxy.Desktop.Core.Services.Settings;

namespace NonProxy.Desktop.Tests;

public sealed class DesktopSettingsServiceTests
{
    [Fact]
    public async Task RoundTripUsesAtomicLocalFileAndNormalizesTheme()
    {
        var directory = Directory.CreateTempSubdirectory("nonproxy-settings-");
        try
        {
            var path = Path.Combine(directory.FullName, "desktop-settings.json");
            using var service = new JsonDesktopSettingsService(
                new DesktopSettingsPath(path));

            Assert.Equal(
                DesktopSettings.Defaults,
                await service.GetAsync(TestContext.Current.CancellationToken));

            await service.SaveAsync(
                new DesktopSettings("Dark"),
                TestContext.Current.CancellationToken);

            Assert.Equal(
                new DesktopSettings("Dark"),
                await service.GetAsync(TestContext.Current.CancellationToken));
            Assert.Empty(Directory.EnumerateFiles(directory.FullName, "*.tmp"));

            await service.SaveAsync(
                new DesktopSettings("Unknown"),
                TestContext.Current.CancellationToken);
            Assert.Equal(
                DesktopSettings.Defaults,
                await service.GetAsync(TestContext.Current.CancellationToken));
        }
        finally
        {
            directory.Delete(recursive: true);
        }
    }

    [Fact]
    public async Task CorruptOrOversizedFileFallsBackWithoutRewritingIt()
    {
        var directory = Directory.CreateTempSubdirectory("nonproxy-settings-");
        try
        {
            var path = Path.Combine(directory.FullName, "desktop-settings.json");
            await File.WriteAllTextAsync(
                path,
                "{not-json",
                TestContext.Current.CancellationToken);
            using var service = new JsonDesktopSettingsService(
                new DesktopSettingsPath(path));

            Assert.Equal(
                DesktopSettings.Defaults,
                await service.GetAsync(TestContext.Current.CancellationToken));
            Assert.Equal(
                "{not-json",
                await File.ReadAllTextAsync(
                    path,
                    TestContext.Current.CancellationToken));

            await File.WriteAllBytesAsync(
                path,
                new byte[(64 * 1024) + 1],
                TestContext.Current.CancellationToken);
            Assert.Equal(
                DesktopSettings.Defaults,
                await service.GetAsync(TestContext.Current.CancellationToken));
        }
        finally
        {
            directory.Delete(recursive: true);
        }
    }
}
