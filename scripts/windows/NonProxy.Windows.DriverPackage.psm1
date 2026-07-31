#requires -Version 7.4

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

if (-not ("NonProxyDriverPackageNative" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

internal static class NonProxyDriverPackageNative {
    [DllImport("newdev.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool DiInstallDriver(
        IntPtr parent, string infPath, uint flags, out bool needReboot);

    [DllImport("newdev.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool DiUninstallDriver(
        IntPtr parent, string infPath, uint flags, out bool needReboot);

    internal static bool Install(string infPath) {
        bool needReboot;
        if (!DiInstallDriver(IntPtr.Zero, infPath, 0, out needReboot)) {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        return needReboot;
    }

    internal static bool Uninstall(string infPath) {
        bool needReboot;
        if (!DiUninstallDriver(IntPtr.Zero, infPath, 0, out needReboot)) {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        return needReboot;
    }
}
"@
}

function Install-NonProxyDriverPackage {
    param(
        [Parameter(Mandatory = $true)]
        [string]$InfPath
    )

    return [NonProxyDriverPackageNative]::Install($InfPath)
}

function Uninstall-NonProxyDriverPackage {
    param(
        [Parameter(Mandatory = $true)]
        [string]$InfPath
    )

    return [NonProxyDriverPackageNative]::Uninstall($InfPath)
}

Export-ModuleMember -Function @(
    "Install-NonProxyDriverPackage",
    "Uninstall-NonProxyDriverPackage"
)
