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

        return LocalAdapterEndpoint.FromWindowsEnvironment(
            LocalAdapterEndpoint.WindowsPipeForUserSid(userSid));
    }
}
