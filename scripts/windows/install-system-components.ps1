#requires -Version 7.4

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet("Query", "Install", "Repair", "Uninstall")]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [string]$PackageRoot,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedPublisherThumbprint,
    [string]$ExitProbeEndpoint,
    [string[]]$ExitProbePublicKeys,
    [switch]$ConfirmSystemMutation,
    [switch]$PurgeUserData,
    [switch]$ConfirmPurgeUserData
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.DriverPackage.psm1") -Force
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Service.psm1") -Force

Assert-NonProxyWindows
if ($Action -notin @("Install", "Repair") -and (
    $PSBoundParameters.ContainsKey("ExitProbeEndpoint") -or
    $PSBoundParameters.ContainsKey("ExitProbePublicKeys")
)) {
    throw "出口探针配置只允许用于 Install 或 Repair。"
}
$Layout = Get-NonProxySystemLayout
$previousExitProbe = Get-NonProxyExitProbeServiceConfiguration -Layout $Layout
$requestedExitProbe = if (
    $PSBoundParameters.ContainsKey("ExitProbeEndpoint") -or
    $PSBoundParameters.ContainsKey("ExitProbePublicKeys")
) {
    [pscustomobject]@{
        Endpoint = $ExitProbeEndpoint
        PublicKeys = @($ExitProbePublicKeys)
    }
} else {
    $previousExitProbe
}

function Copy-VersionedPayload {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Package
    )

    $instance = [Guid]::NewGuid().ToString("N").Substring(0, 12)
    $leaf = "$($Package.version)-$($Package.architecture)-$instance"
    if ($leaf -notmatch "^[0-9A-Za-z.+-]+-(x64|arm64)-[0-9a-f]{12}$") {
        throw "版本化安装目录名称无效。"
    }
    $destination = Join-Path $Layout.ProgramRoot $leaf
    $staging = Join-Path $Layout.ProgramRoot (
        ".staging-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $staging | Out-Null
    try {
        Copy-Item -Path (
            Join-Path $Package.packageRoot "*") `
            -Destination $staging -Recurse
        Move-Item -LiteralPath $staging -Destination $destination
        [void](& (Join-Path $PSScriptRoot "verify-release-package.ps1") `
            -PackageRoot $destination `
            -ExpectedPublisherThumbprint $Package.publisherThumbprint `
            -PassThru)
    } catch {
        if (Test-Path -LiteralPath $staging) {
            Remove-Item -LiteralPath $staging -Recurse -Force
        }
        if (Test-Path -LiteralPath $destination) {
            Remove-Item -LiteralPath $destination -Recurse -Force
        }
        throw
    }
    return $destination
}

function Install-SystemComponents {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Package
    )

    $previous = Get-NonProxyInstalledMetadata -Layout $Layout
    $previousPublisher = Get-NonProxyInstalledMetadataValue `
        $previous "PublisherThumbprint"
    if (-not [string]::IsNullOrWhiteSpace($previousPublisher) -and
        $previousPublisher -ne [string]$Package.publisherThumbprint) {
        throw "升级包发布者与已安装发布者不一致，拒绝静默证书切换。"
    }
    $installRoot = Copy-VersionedPayload -Package $Package
    $driverInf = Join-Path $installRoot "driver\NonProxyWfp.inf"
    $gateway = Join-Path $installRoot "service\nonproxy-gatewayd.exe"
    $gatewayFingerprint = Get-NonProxyFileSha256 -Path $gateway
    $requiresReboot = $false
    try {
        Stop-NonProxyProductService -Layout $Layout
        Protect-NonProxyStateDirectory -Layout $Layout
        $requiresReboot = Install-NonProxyDriverPackage -InfPath $driverInf
        Set-NonProxyProductService `
            -Layout $Layout `
            -Executable $gateway `
            -Fingerprint $gatewayFingerprint `
            -ExitProbeEndpoint $requestedExitProbe.Endpoint `
            -ExitProbePublicKeys $requestedExitProbe.PublicKeys
        New-Item -Path $Layout.RegistryPath -Force | Out-Null
        New-ItemProperty -LiteralPath $Layout.RegistryPath `
            -Name InstallRoot -Value $installRoot -PropertyType String -Force |
            Out-Null
        New-ItemProperty -LiteralPath $Layout.RegistryPath `
            -Name Version -Value $Package.version -PropertyType String -Force |
            Out-Null
        New-ItemProperty -LiteralPath $Layout.RegistryPath `
            -Name Architecture -Value $Package.architecture `
            -PropertyType String -Force | Out-Null
        New-ItemProperty -LiteralPath $Layout.RegistryPath `
            -Name PublisherThumbprint -Value $Package.publisherThumbprint `
            -PropertyType String -Force | Out-Null
        if (-not $requiresReboot) {
            Start-Service -Name $Layout.ServiceName
            (Get-Service -Name $Layout.ServiceName).WaitForStatus(
                "Running",
                [TimeSpan]::FromSeconds(30))
        }
    } catch {
        $installError = $_
        $oldRoot = Get-NonProxyInstalledMetadataValue `
            $previous "InstallRoot"
        try {
            if (-not [string]::IsNullOrWhiteSpace($oldRoot) -and
                (Test-Path -LiteralPath $oldRoot)) {
                $oldInf = Join-Path $oldRoot "driver\NonProxyWfp.inf"
                $oldGateway = Join-Path $oldRoot "service\nonproxy-gatewayd.exe"
                $rollbackNeedsReboot = Install-NonProxyDriverPackage `
                    -InfPath $oldInf
                Set-NonProxyProductService `
                    -Layout $Layout `
                    -Executable $oldGateway `
                    -Fingerprint (Get-NonProxyFileSha256 -Path $oldGateway) `
                    -ExitProbeEndpoint $previousExitProbe.Endpoint `
                    -ExitProbePublicKeys $previousExitProbe.PublicKeys
                New-Item -Path $Layout.RegistryPath -Force | Out-Null
                foreach ($name in @(
                    "InstallRoot",
                    "Version",
                    "Architecture",
                    "PublisherThumbprint"
                )) {
                    $oldValue = Get-NonProxyInstalledMetadataValue `
                        $previous $name
                    if ($null -ne $oldValue) {
                        New-ItemProperty -LiteralPath $Layout.RegistryPath `
                            -Name $name `
                            -Value $oldValue `
                            -PropertyType String `
                            -Force | Out-Null
                    }
                }
                if (-not $rollbackNeedsReboot) {
                    Start-Service -Name $Layout.ServiceName
                } else {
                    Write-Warning "旧驱动已恢复，但需要用户安排重启后才能重新启用。"
                }
            } else {
                Remove-NonProxyProductService -Layout $Layout
                [void](Uninstall-NonProxyDriverPackage -InfPath $driverInf)
            }
        } catch {
            throw (
                "安装失败且自动回滚失败，需要人工恢复。" +
                "安装错误：$($installError.Exception.Message)；" +
                "回滚错误：$($_.Exception.Message)")
        }
        throw $installError
    }
    return $requiresReboot
}

function Uninstall-SystemComponents {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Package
    )

    $metadata = Get-NonProxyInstalledMetadata -Layout $Layout
    $requiresReboot = $false
    Stop-NonProxyProductService -Layout $Layout
    $installRoot = Get-NonProxyInstalledMetadataValue `
        $metadata "InstallRoot"
    $driverInf = if (-not [string]::IsNullOrWhiteSpace($installRoot)) {
        Join-Path $installRoot "driver\NonProxyWfp.inf"
    } else {
        Join-Path $Package.packageRoot "driver\NonProxyWfp.inf"
    }
    if (Test-Path -LiteralPath $driverInf -PathType Leaf) {
        try {
            $requiresReboot = Uninstall-NonProxyDriverPackage -InfPath $driverInf
        } catch {
            try {
                Start-Service -Name $Layout.ServiceName -ErrorAction Stop
            } catch {
                Write-Warning "驱动卸载失败后无法重新启动旧 Service。"
            }
            throw
        }
    }
    Remove-NonProxyProductService -Layout $Layout
    if ($null -ne $metadata -and
        -not [string]::IsNullOrWhiteSpace($installRoot)) {
        if ($installRoot.StartsWith(
            ([IO.Path]::GetFullPath($Layout.ProgramRoot) + "\"),
            [StringComparison]::OrdinalIgnoreCase) -and
            (Test-Path -LiteralPath $installRoot)) {
            Remove-Item -LiteralPath $installRoot -Recurse -Force
        }
        Remove-Item -LiteralPath $Layout.RegistryPath -Recurse -Force
    }
    if ($PurgeUserData) {
        if (([IO.Path]::GetFullPath($Layout.StateDirectory)) -eq
            ([IO.Path]::GetFullPath(
                (Join-Path $env:ProgramData "NonProxy"))) -and
            (Test-Path -LiteralPath $Layout.StateDirectory)) {
            Remove-Item -LiteralPath $Layout.StateDirectory -Recurse -Force
        }
    }
    return $requiresReboot
}

$package = & (Join-Path $PSScriptRoot "verify-release-package.ps1") `
    -PackageRoot $PackageRoot `
    -ExpectedPublisherThumbprint $ExpectedPublisherThumbprint `
    -PassThru

if ($Action -eq "Query") {
    (Get-NonProxySystemState -Layout $Layout -PackageTrusted $true) |
        ConvertTo-Json -Depth 6 -Compress
    return
}

Assert-NonProxySystemMutation -Confirmed $ConfirmSystemMutation.IsPresent
if ($Action -eq "Uninstall" -and $PurgeUserData -and
    (-not $ConfirmPurgeUserData -or
        $env:NONPROXY_CONFIRM_WINDOWS_DATA_PURGE -ne "1")) {
    throw (
        "清除用户数据还必须传入 -ConfirmPurgeUserData，" +
        "并设置 NONPROXY_CONFIRM_WINDOWS_DATA_PURGE=1。")
}
$requiresReboot = if ($Action -eq "Uninstall") {
    Uninstall-SystemComponents -Package $package
} else {
    Install-SystemComponents -Package $package
}
$result = Get-NonProxySystemState -Layout $Layout -PackageTrusted $true
$result["action"] = $Action
$result["requiresReboot"] = $requiresReboot
if ($requiresReboot) {
    $result["message"] = "系统组件变更已提交，需要由用户选择时间重启 Windows。"
}
$result | ConvertTo-Json -Depth 6 -Compress
