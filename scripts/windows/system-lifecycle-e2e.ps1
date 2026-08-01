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
    [Parameter(Mandatory = $true)]
    [string]$EvidenceDirectory,
    [string]$ExitProbeEndpoint,
    [string[]]$ExitProbePublicKeys,
    [switch]$ConfirmSystemMutation
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

Assert-NonProxyWindows
if ($Action -notin @("Install", "Repair") -and (
    $PSBoundParameters.ContainsKey("ExitProbeEndpoint") -or
    $PSBoundParameters.ContainsKey("ExitProbePublicKeys")
)) {
    throw "出口探针配置只允许用于 Install 或 Repair。"
}
$evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
if (Test-Path -LiteralPath $evidenceRoot) {
    $existing = Get-ChildItem -LiteralPath $evidenceRoot -Force |
        Select-Object -First 1
    if ($null -ne $existing) {
        throw "证据目录必须不存在或为空，拒绝覆盖：$evidenceRoot"
    }
}
New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null

function Write-EvidenceJson {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [object]$Value
    )

    $Value | ConvertTo-Json -Depth 10 |
        Set-Content -LiteralPath (Join-Path $evidenceRoot $Name) -Encoding UTF8
}

function Get-HostEvidence {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    return [ordered]@{
        capturedUtc = [DateTime]::UtcNow.ToString("o")
        action = $Action
        computerName = $env:COMPUTERNAME
        osVersion = [Environment]::OSVersion.VersionString
        osBuild = [Environment]::OSVersion.Version.Build
        processArchitecture = $env:PROCESSOR_ARCHITECTURE
        nativeArchitecture = $env:PROCESSOR_ARCHITEW6432
        userSid = $identity.User.Value
        elevated = Test-NonProxyAdministrator
        mutationEnvironmentEnabled = (
            $env:NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION -eq "1")
    }
}

function Invoke-StateQuery {
    $output = & (
        Join-Path $PSScriptRoot "install-system-components.ps1") `
        Query `
        -PackageRoot $PackageRoot `
        -ExpectedPublisherThumbprint $ExpectedPublisherThumbprint
    return ($output | Select-Object -Last 1 | ConvertFrom-Json)
}

function Write-SystemSnapshot {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Prefix
    )

    Write-EvidenceJson "$Prefix-state.json" (Invoke-StateQuery)
    Write-EvidenceJson "$Prefix-services.json" @(
        Get-CimInstance Win32_Service |
            Where-Object { $_.Name -in @("NonProxyGateway", "BFE") } |
            Select-Object Name, State, StartMode, PathName, ProcessId
        Get-CimInstance Win32_SystemDriver |
            Where-Object { $_.Name -eq "NonProxyWfp" } |
            Select-Object Name, State, StartMode, PathName
    )
    Write-EvidenceJson "$Prefix-adapter-task.json" @(
        Get-ScheduledTask -TaskName "NonProxyAdapterHost" `
            -TaskPath "\" `
            -ErrorAction SilentlyContinue |
            Select-Object TaskName, State, Actions, Triggers, Principal, Settings
    )
    Write-EvidenceJson "$Prefix-adapter-processes.json" @(
        Get-CimInstance Win32_Process -Filter `
            "Name = 'nonproxy-adapter-host.exe'" |
            Select-Object ProcessId, SessionId, ExecutablePath
    )
    Write-EvidenceJson "$Prefix-adapters.json" @(
        Get-NetAdapter -IncludeHidden |
            Select-Object Name, InterfaceDescription, InterfaceGuid,
                InterfaceIndex, Status, MacAddress, LinkSpeed
    )
    Write-EvidenceJson "$Prefix-routes.json" @(
        Get-NetRoute -AddressFamily IPv4, IPv6 |
            Select-Object AddressFamily, DestinationPrefix, NextHop,
                InterfaceIndex, RouteMetric, Protocol, State
    )
}

function Write-EvidenceManifest {
    $entries = foreach ($file in (
        Get-ChildItem -LiteralPath $evidenceRoot -File |
            Sort-Object Name)) {
        [ordered]@{
            path = $file.Name
            size = $file.Length
            sha256 = Get-NonProxyFileSha256 -Path $file.FullName
        }
    }
    Write-EvidenceJson "evidence-sha256.json" ([ordered]@{
        schemaVersion = 1
        createdUtc = [DateTime]::UtcNow.ToString("o")
        files = @($entries)
    })
}

try {
    Write-EvidenceJson "host.json" (Get-HostEvidence)
    $package = & (
        Join-Path $PSScriptRoot "verify-release-package.ps1") `
        -PackageRoot $PackageRoot `
        -ExpectedPublisherThumbprint $ExpectedPublisherThumbprint `
        -PassThru
    Write-EvidenceJson "package.json" $package
    Write-SystemSnapshot "before"

    if ($Action -ne "Query") {
        Assert-NonProxySystemMutation `
            -Confirmed $ConfirmSystemMutation.IsPresent
        $arguments = @{
            PackageRoot = $PackageRoot
            ExpectedPublisherThumbprint = $ExpectedPublisherThumbprint
            ConfirmSystemMutation = $true
        }
        if ($PSBoundParameters.ContainsKey("ExitProbeEndpoint")) {
            $arguments["ExitProbeEndpoint"] = $ExitProbeEndpoint
        }
        if ($PSBoundParameters.ContainsKey("ExitProbePublicKeys")) {
            $arguments["ExitProbePublicKeys"] = $ExitProbePublicKeys
        }
        $output = & (
            Join-Path $PSScriptRoot "install-system-components.ps1") `
            $Action @arguments
        $operation = $output | Select-Object -Last 1 | ConvertFrom-Json
        Write-EvidenceJson "operation.json" $operation
    }

    Write-SystemSnapshot "after"
    Write-EvidenceJson "result.json" ([ordered]@{
        success = $true
        action = $Action
        completedUtc = [DateTime]::UtcNow.ToString("o")
    })
    Write-EvidenceManifest
} catch {
    Write-EvidenceJson "result.json" ([ordered]@{
        success = $false
        action = $Action
        completedUtc = [DateTime]::UtcNow.ToString("o")
        errorType = $_.Exception.GetType().FullName
        message = $_.Exception.Message
    })
    Write-EvidenceManifest
    throw
}

Write-Host "Windows 系统生命周期证据已保存：$evidenceRoot"
