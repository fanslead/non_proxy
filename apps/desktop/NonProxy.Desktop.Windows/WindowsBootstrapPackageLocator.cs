using System.Runtime.Versioning;
using NonProxy.Windows.Security;

namespace NonProxy.Desktop.Windows;

internal sealed class WindowsBootstrapPackage(
    string packageRoot,
    string bootstrapExecutable,
    FileStream executableLease) : IDisposable
{
    public string PackageRoot { get; } = packageRoot;

    public string BootstrapExecutable { get; } = bootstrapExecutable;

    public void Dispose()
    {
        executableLease.Dispose();
    }
}

internal interface IWindowsBootstrapPackageLocator
{
    WindowsBootstrapPackage Locate();
}

[SupportedOSPlatform("windows")]
internal sealed class WindowsBootstrapPackageLocator :
    IWindowsBootstrapPackageLocator
{
    public WindowsBootstrapPackage Locate()
    {
        var expectedPublisher = CompiledWindowsPublisherIdentity.Read(
            typeof(WindowsBootstrapPackageLocator).Assembly)
            ?? throw new WindowsBootstrapException(
                "当前构建没有编译固定的 Windows 发布者身份。",
                "NP_WINDOWS_PUBLISHER_NOT_CONFIGURED");
        var desktopDirectory = Path.GetFullPath(AppContext.BaseDirectory)
            .TrimEnd(Path.DirectorySeparatorChar);
        if (!string.Equals(
                Path.GetFileName(desktopDirectory),
                "desktop",
                StringComparison.OrdinalIgnoreCase))
        {
            throw new WindowsBootstrapException(
                "Windows 桌面程序不在受支持的发布包结构中。",
                "NP_WINDOWS_PACKAGE_LAYOUT_INVALID");
        }
        var packageRoot = Path.GetDirectoryName(desktopDirectory)
            ?? throw new WindowsBootstrapException(
                "无法定位 Windows 发布包根目录。",
                "NP_WINDOWS_PACKAGE_LAYOUT_INVALID");
        var bootstrapDirectory = Path.Combine(packageRoot, "bootstrap");
        var executable = Path.Combine(
            bootstrapDirectory,
            "NonProxy.Windows.Bootstrap.exe");
        foreach (var path in new[]
        {
            packageRoot,
            desktopDirectory,
            bootstrapDirectory,
            executable,
        })
        {
            AssertRegularPath(path);
        }
        var lease = new FileStream(
            executable,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read);
        try
        {
            if (lease.Length <= 0)
            {
                throw new WindowsBootstrapException(
                    "Windows 安装 Bootstrap 文件为空。",
                    "NP_WINDOWS_BOOTSTRAP_UNTRUSTED");
            }
            var signer = WindowsAuthenticodeTrust.VerifyFile(executable);
            if (signer?.CertificateSha256 != expectedPublisher)
            {
                throw new WindowsBootstrapException(
                    "Windows 安装 Bootstrap 的发布者不受信任。",
                    "NP_WINDOWS_BOOTSTRAP_UNTRUSTED");
            }
            return new WindowsBootstrapPackage(packageRoot, executable, lease);
        }
        catch
        {
            lease.Dispose();
            throw;
        }
    }

    private static void AssertRegularPath(string path)
    {
        if (!File.Exists(path) && !Directory.Exists(path))
        {
            throw new WindowsBootstrapException(
                "Windows 发布包缺少安装 Bootstrap。",
                "NP_WINDOWS_BOOTSTRAP_NOT_PACKAGED");
        }
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
        {
            throw new WindowsBootstrapException(
                "Windows 发布包信任路径不允许重解析点。",
                "NP_WINDOWS_PACKAGE_REPARSE_POINT");
        }
    }
}

internal sealed class WindowsBootstrapException(
    string message,
    string errorCode,
    Exception? innerException = null) : Exception(message, innerException)
{
    public string ErrorCode { get; } = errorCode;
}
