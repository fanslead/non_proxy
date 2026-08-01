using NonProxy.Desktop.Core.Services.Control.Transport;

namespace NonProxy.Desktop.Windows;

internal static class WindowsControlEndpointFactory
{
    public static LocalControlEndpoint Create()
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException(
                "Windows 控制端点只能在 Windows 系统中创建。");
        }

        var commonApplicationData = Environment.GetFolderPath(
            Environment.SpecialFolder.CommonApplicationData);
        return CreateForMachine(commonApplicationData);
    }

    internal static LocalControlEndpoint CreateForMachine(
        string commonApplicationData)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(commonApplicationData);
        if (!Path.IsPathFullyQualified(commonApplicationData))
        {
            throw new ArgumentException(
                "Windows 公共应用数据目录必须是绝对路径。",
                nameof(commonApplicationData));
        }

        return LocalControlEndpoint.FromWindowsStateDirectory(
            Path.Combine(commonApplicationData, "NonProxy"));
    }
}
