using System.Security.Principal;
using NonProxy.Desktop.Core.Services.Adapters.Transport;

namespace NonProxy.Desktop.Windows;

internal static class WindowsAdapterEndpointFactory
{
    public static LocalAdapterEndpoint Create()
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException(
                "Windows Adapter 端点只能在 Windows 用户会话中创建。");
        }

        using var identity = WindowsIdentity.GetCurrent();
        var userSid = identity.User?.Value;
        if (string.IsNullOrWhiteSpace(userSid))
        {
            throw new InvalidOperationException("无法读取当前 Windows 用户 SID。");
        }

        var localApplicationData = Environment.GetFolderPath(
            Environment.SpecialFolder.LocalApplicationData);
        return CreateForUser(localApplicationData, userSid);
    }

    internal static LocalAdapterEndpoint CreateForUser(
        string localApplicationData,
        string userSid)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localApplicationData);
        ArgumentException.ThrowIfNullOrWhiteSpace(userSid);
        if (!Path.IsPathFullyQualified(localApplicationData))
        {
            throw new ArgumentException(
                "Windows 用户应用数据目录必须是绝对路径。",
                nameof(localApplicationData));
        }

        var stateDirectory = Path.Combine(
            localApplicationData,
            "NonProxy",
            "adapter-host");
        return LocalAdapterEndpoint.FromWindowsStateDirectory(
            stateDirectory,
            LocalAdapterEndpoint.WindowsPipeForUserSid(userSid));
    }
}
