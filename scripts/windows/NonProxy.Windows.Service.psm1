#requires -Version 7.4

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

function Get-NonProxySystemLayout {
    return [pscustomobject]@{
        ServiceName = "NonProxyGateway"
        DriverName = "NonProxyWfp"
        RegistryPath = "HKLM:\Software\NonProxy\System"
        StateDirectory = Join-Path $env:ProgramData "NonProxy"
        ProgramRoot = Join-Path $env:ProgramFiles "NonProxy\system"
        PipeSddl = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)"
    }
}

function Get-NonProxyInstalledMetadata {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout
    )

    if (-not (Test-Path -LiteralPath $Layout.RegistryPath)) {
        return $null
    }
    return Get-ItemProperty -LiteralPath $Layout.RegistryPath
}

function Get-NonProxyInstalledMetadataValue {
    param(
        [object]$Metadata,
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if ($null -eq $Metadata) {
        return $null
    }
    $property = $Metadata.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return [string]$property.Value
}

function Get-NonProxySystemState {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout,
        [Parameter(Mandatory = $true)]
        [bool]$PackageTrusted
    )

    $service = Get-NonProxyServiceSnapshot -Name $Layout.ServiceName
    $driver = Get-NonProxyServiceSnapshot -Name $Layout.DriverName
    $metadata = Get-NonProxyInstalledMetadata -Layout $Layout
    $installed = $service.installed -and $driver.installed
    $absent = -not $service.installed -and -not $driver.installed
    $status = if (
        $installed -and
        $service.status -eq "Running" -and
        $driver.status -eq "Running") {
        "Installed"
    } elseif ($absent) {
        "NotInstalled"
    } else {
        "Partial"
    }
    $message = switch ($status) {
        "Installed" { "Windows Service 与 WFP Driver 已安装并运行。" }
        "NotInstalled" { "Windows 系统组件尚未安装。" }
        default { "Windows 系统组件仅部分就绪，需要修复。" }
    }
    return [ordered]@{
        success = $true
        action = "Query"
        status = $status
        message = $message
        errorCode = if ($status -eq "Partial") {
            "NP_WINDOWS_COMPONENT_PARTIAL"
        } else {
            $null
        }
        requiresReboot = $false
        packageTrusted = $PackageTrusted
        version = Get-NonProxyInstalledMetadataValue $metadata "Version"
        architecture = Get-NonProxyInstalledMetadataValue $metadata "Architecture"
        steps = @(
            [ordered]@{
                id = "gateway"
                name = "后台服务"
                installed = $service.installed
                status = $service.status
            },
            [ordered]@{
                id = "wfp-driver"
                name = "WFP Driver"
                installed = $driver.installed
                status = $driver.status
            }
        )
    }
}

function Stop-NonProxyProductService {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout
    )

    $service = Get-Service `
        -Name $Layout.ServiceName `
        -ErrorAction SilentlyContinue
    if ($null -ne $service -and $service.Status -ne "Stopped") {
        Stop-Service -Name $Layout.ServiceName -Force
        $service.WaitForStatus("Stopped", [TimeSpan]::FromSeconds(30))
    }
}

function Remove-NonProxyProductService {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout
    )

    Stop-NonProxyProductService -Layout $Layout
    $service = Get-Service `
        -Name $Layout.ServiceName `
        -ErrorAction SilentlyContinue
    if ($null -ne $service) {
        Invoke-NonProxyExternal -FilePath "$env:SystemRoot\System32\sc.exe" `
            -Arguments @("delete", $Layout.ServiceName) | Out-Null
    }
}

function Set-NonProxyProductService {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout,
        [Parameter(Mandatory = $true)]
        [string]$Executable,
        [Parameter(Mandatory = $true)]
        [string]$Fingerprint
    )

    $binaryPath = "`"$Executable`""
    $existing = Get-Service `
        -Name $Layout.ServiceName `
        -ErrorAction SilentlyContinue
    if ($null -eq $existing) {
        New-Service `
            -Name $Layout.ServiceName `
            -BinaryPathName $binaryPath `
            -DisplayName "NonProxy Gateway" `
            -Description "NonProxy 本地策略、DNS 与流量网关" `
            -StartupType Automatic `
            -DependsOn @("BFE", $Layout.DriverName) | Out-Null
    } else {
        Stop-NonProxyProductService -Layout $Layout
        Invoke-NonProxyExternal -FilePath "$env:SystemRoot\System32\sc.exe" `
            -Arguments @(
                "config", $Layout.ServiceName,
                "binPath=", $binaryPath,
                "start=", "auto",
                "depend=", "BFE/$($Layout.DriverName)"
            ) | Out-Null
    }

    $serviceRegistry = (
        "HKLM:\SYSTEM\CurrentControlSet\Services\$($Layout.ServiceName)")
    $environment = @(
        "NONPROXY_STATE_DIR=$($Layout.StateDirectory)",
        "NONPROXY_WINDOWS_CONTROL_PIPE=\\.\pipe\NonProxy.Control.v1",
        "NONPROXY_WINDOWS_FLOW_PIPE=\\.\pipe\NonProxy.Flow.v1",
        "NONPROXY_WINDOWS_PIPE_SDDL=$($Layout.PipeSddl)",
        "NONPROXY_GATEWAY_BUNDLE_FINGERPRINT=$Fingerprint"
    )
    New-ItemProperty -LiteralPath $serviceRegistry `
        -Name Environment `
        -PropertyType MultiString `
        -Value $environment `
        -Force | Out-Null
    Invoke-NonProxyExternal -FilePath "$env:SystemRoot\System32\sc.exe" `
        -Arguments @(
            "sidtype",
            $Layout.ServiceName,
            "unrestricted"
        ) | Out-Null
    Invoke-NonProxyExternal -FilePath "$env:SystemRoot\System32\sc.exe" `
        -Arguments @(
            "failure", $Layout.ServiceName,
            "reset=", "86400",
            "actions=", "restart/5000/restart/15000/none/0"
        ) | Out-Null
}

function Protect-NonProxyStateDirectory {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout
    )

    New-Item -ItemType Directory -Force `
        -Path $Layout.StateDirectory | Out-Null
    $userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    Invoke-NonProxyExternal -FilePath "$env:SystemRoot\System32\icacls.exe" `
        -Arguments @(
            $Layout.StateDirectory,
            "/inheritance:r",
            "/grant:r",
            "*S-1-5-18:(OI)(CI)(F)",
            "*S-1-5-32-544:(OI)(CI)(F)",
            "*${userSid}:(OI)(CI)(RX)"
        ) | Out-Null
}

Export-ModuleMember -Function @(
    "Get-NonProxyInstalledMetadata",
    "Get-NonProxyInstalledMetadataValue",
    "Get-NonProxySystemLayout",
    "Get-NonProxySystemState",
    "Protect-NonProxyStateDirectory",
    "Remove-NonProxyProductService",
    "Set-NonProxyProductService",
    "Stop-NonProxyProductService"
)
