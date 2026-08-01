using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Windows.ApplicationCatalog;

namespace NonProxy.Desktop.Tests;

public sealed class WindowsApplicationCatalogTests
{
    [Fact]
    public void StableIdentityDecoderMatchesWfpUtf16Normalization()
    {
        var bytes = "\\device\\harddiskvolume4\\apps\\office.exe\0"
            .SelectMany(character => BitConverter.GetBytes(character))
            .ToArray();

        var identity = WindowsApplicationStableIdentity.Decode(bytes);

        Assert.Equal(
            "\\device\\harddiskvolume4\\apps\\office.exe",
            identity);
        Assert.Null(WindowsApplicationStableIdentity.Decode([1]));
        Assert.Null(WindowsApplicationStableIdentity.Decode(
            [0x61, 0x00, 0x00, 0x00, 0x62, 0x00]));
        Assert.Null(WindowsApplicationStableIdentity.Decode([0x00, 0xd8]));
    }

    [Fact]
    public void PackageIdentityDecoderRequiresCanonicalApplicationPackageSid()
    {
        var sid = PackageSid(2, 1, 2, 3, 4, 5, 6, 7);

        Assert.Equal(
            "package-sid:S-1-15-2-1-2-3-4-5-6-7",
            WindowsPackageStableIdentity.StableIdentity(sid));
        Assert.Equal(
            "package-publisher-id:8wekyb3d8bbwe",
            WindowsPackageStableIdentity.SignerIdentity("8WEKYB3D8BBWE"));
        Assert.Null(WindowsPackageStableIdentity.StableIdentity(
            PackageSid(3, 1, 2, 3, 4, 5, 6, 7)));
        Assert.Null(WindowsPackageStableIdentity.StableIdentity(
            sid.AsSpan(0, sid.Length - 1)));
        Assert.Null(WindowsPackageStableIdentity.SignerIdentity("publisher"));
        Assert.Null(WindowsPackageStableIdentity.SignerIdentity("8wekyb3d8bbw_"));
        Assert.Null(WindowsPackageStableIdentity.SignerIdentity(" 8wekyb3d8bbwe"));
    }

    [Fact]
    public async Task CatalogKeepsOnlyTrustedIdentitiesAndMergesRunningState()
    {
        var discovery = new FixedDiscovery(
            new("C:\\Apps\\Office.exe", "Office", false),
            new("C:\\Apps\\Office-running.exe", "Office", true),
            new("C:\\Apps\\Unsigned.exe", "Unsigned", true));
        var identity = new FixedIdentityReader(candidate => candidate.DisplayName switch
        {
            "Unsigned" => null,
            _ => Application(candidate.DisplayName, candidate.IsRunning),
        });
        var catalog = new WindowsApplicationCatalog(
            discovery,
            identity,
            new FixedPicker(null));

        var snapshot = await catalog.ListAsync(
            TestContext.Current.CancellationToken);

        var application = Assert.Single(snapshot.Applications);
        Assert.True(snapshot.IsAvailable);
        Assert.True(snapshot.CanChooseApplication);
        Assert.True(application.IsRunning);
        Assert.False(application.IncludeHelpers);
        Assert.Contains("1 个可信应用", snapshot.Message, StringComparison.Ordinal);
        Assert.Contains("2 个项目", snapshot.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task CatalogReportsPackageDiscoveryFailureWithoutHidingWin32Apps()
    {
        var discovery = new FixedDiscovery(
            [new("C:\\Apps\\Office.exe", "Office", false)],
            false);
        var catalog = new WindowsApplicationCatalog(
            discovery,
            new FixedIdentityReader(candidate => Application(
                candidate.DisplayName,
                candidate.IsRunning)),
            new FixedPicker(null));

        var snapshot = await catalog.ListAsync(
            TestContext.Current.CancellationToken);

        Assert.Single(snapshot.Applications);
        Assert.True(snapshot.IsAvailable);
        Assert.Contains("打包应用目录暂时不可用", snapshot.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task PickerRejectsUnsignedExecutableWithoutCreatingASelection()
    {
        var catalog = new WindowsApplicationCatalog(
            new FixedDiscovery(),
            new FixedIdentityReader(_ => null),
            new FixedPicker("C:\\Apps\\Unsigned.exe"));

        var result = await catalog.ChooseAsync(
            TestContext.Current.CancellationToken);

        Assert.False(result.Succeeded);
        Assert.Null(result.Application);
        Assert.Contains("可信 Authenticode 签名", result.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task PickerFailureReturnsSafeUnavailableSelection()
    {
        var catalog = new WindowsApplicationCatalog(
            new FixedDiscovery(),
            new FixedIdentityReader(_ => throw new InvalidOperationException()),
            new ThrowingPicker());

        var result = await catalog.ChooseAsync(
            TestContext.Current.CancellationToken);

        Assert.False(result.Succeeded);
        Assert.Null(result.Application);
        Assert.Contains("未创建规则", result.Message, StringComparison.Ordinal);
    }

    private static ApplicationCatalogEntry Application(
        string displayName,
        bool isRunning)
    {
        return new(
            displayName,
            "\\device\\harddiskvolume4\\apps\\office.exe",
            $"cert-sha256:{new string('a', 64)}",
            null,
            isRunning,
            null,
            false);
    }

    private static byte[] PackageSid(params uint[] subAuthorities)
    {
        var sid = new byte[8 + (subAuthorities.Length * sizeof(uint))];
        sid[0] = 1;
        sid[1] = checked((byte)subAuthorities.Length);
        sid[7] = 15;
        for (var index = 0; index < subAuthorities.Length; index++)
        {
            BitConverter.GetBytes(subAuthorities[index])
                .CopyTo(sid, 8 + (index * sizeof(uint)));
        }
        return sid;
    }

    private sealed class FixedDiscovery : IWindowsApplicationDiscovery
    {
        private readonly bool _packageCatalogAvailable;
        private readonly WindowsApplicationCandidate[] _candidates;

        public FixedDiscovery(params WindowsApplicationCandidate[] candidates)
            : this(candidates, true)
        {
        }

        public FixedDiscovery(
            WindowsApplicationCandidate[] candidates,
            bool packageCatalogAvailable)
        {
            _packageCatalogAvailable = packageCatalogAvailable;
            _candidates = candidates;
        }

        public WindowsApplicationDiscoverySnapshot Discover(
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return new WindowsApplicationDiscoverySnapshot(
                _candidates,
                _packageCatalogAvailable);
        }
    }

    private sealed class FixedIdentityReader(
        Func<WindowsApplicationCandidate, ApplicationCatalogEntry?> read)
        : IWindowsApplicationIdentityReader
    {
        public ApplicationCatalogEntry? Read(WindowsApplicationCandidate candidate)
        {
            return read(candidate);
        }
    }

    private sealed class FixedPicker(string? path) : IWindowsExecutablePicker
    {
        public Task<string?> PickAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(path);
        }
    }

    private sealed class ThrowingPicker : IWindowsExecutablePicker
    {
        public Task<string?> PickAsync(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            throw new InvalidOperationException("picker unavailable");
        }
    }
}
