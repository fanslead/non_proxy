#requires -Version 7.4

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern("^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$")]
    [string]$Version,
    [Parameter(Mandatory = $true)]
    [ValidateSet("x64", "arm64")]
    [string]$Architecture,
    [Parameter(Mandatory = $true)]
    [string]$DesktopPublishDirectory,
    [Parameter(Mandatory = $true)]
    [string]$GatewayExecutable,
    [Parameter(Mandatory = $true)]
    [string]$AdapterHostExecutable,
    [Parameter(Mandatory = $true)]
    [string]$DriverDirectory,
    [string]$OutputDirectory
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

$desktopSource = Resolve-NonProxyExistingPath `
    -Path $DesktopPublishDirectory -PathType Container
$gatewaySource = Resolve-NonProxyExistingPath `
    -Path $GatewayExecutable -PathType Leaf
$adapterHostSource = Resolve-NonProxyExistingPath `
    -Path $AdapterHostExecutable -PathType Leaf
$driverSource = Resolve-NonProxyExistingPath `
    -Path $DriverDirectory -PathType Container
$desktopExecutable = Join-Path $desktopSource "NonProxy.Desktop.Windows.exe"
if (-not (Test-Path -LiteralPath $desktopExecutable -PathType Leaf)) {
    throw "桌面发布目录缺少 NonProxy.Desktop.Windows.exe。"
}

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repositoryRoot (
        ".artifacts/windows-release/$Version/$Architecture")
}
$packageRoot = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $packageRoot) {
    $existing = Get-ChildItem -LiteralPath $packageRoot -Force |
        Select-Object -First 1
    if ($null -ne $existing) {
        throw "输出目录不是空目录，拒绝覆盖：$packageRoot"
    }
}

$desktopDestination = Join-Path $packageRoot "desktop"
$serviceDestination = Join-Path $packageRoot "service"
$adapterDestination = Join-Path $packageRoot "adapter"
$driverDestination = Join-Path $packageRoot "driver"
$toolsDestination = Join-Path $packageRoot "tools"
New-Item -ItemType Directory -Force -Path @(
    $desktopDestination,
    $serviceDestination,
    $adapterDestination,
    $driverDestination,
    $toolsDestination
) | Out-Null

Copy-Item -Path (Join-Path $desktopSource "*") `
    -Destination $desktopDestination -Recurse -Force
Copy-Item -LiteralPath $gatewaySource `
    -Destination (Join-Path $serviceDestination "nonproxy-gatewayd.exe")
Copy-Item -LiteralPath $adapterHostSource `
    -Destination (Join-Path $adapterDestination "nonproxy-adapter-host.exe")

foreach ($driverName in @(
    "NonProxyWfp.inf",
    "NonProxyWfp.sys",
    "NonProxyWfp.cat"
)) {
    $source = Join-Path $driverSource $driverName
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "驱动目录缺少 $driverName。"
    }
    Copy-Item -LiteralPath $source -Destination $driverDestination
}

foreach ($toolName in @(
    "NonProxy.Windows.Common.psm1",
    "NonProxy.Windows.AdapterHost.psm1",
    "NonProxy.Windows.DriverPackage.psm1",
    "NonProxy.Windows.Service.psm1",
    "verify-release-package.ps1",
    "install-system-components.ps1",
    "driver-verifier.ps1",
    "system-lifecycle-e2e.ps1"
)) {
    $source = Join-Path $PSScriptRoot $toolName
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "仓库缺少发布工具 $toolName。"
    }
    Copy-Item -LiteralPath $source -Destination $toolsDestination
}

$metadata = [ordered]@{
    schemaVersion = 1
    product = "NonProxy"
    version = $Version
    architecture = $Architecture
    minimumWindowsBuild = 18362
    createdUtc = [DateTime]::UtcNow.ToString("o")
}
$metadata | ConvertTo-Json -Depth 4 |
    Set-Content -LiteralPath (
        Join-Path $packageRoot "release-metadata.json") -Encoding UTF8

Write-Host "未签名 Windows 发布目录已生成：$packageRoot"
Write-Host "下一步必须使用 sign-release-package.ps1 生成受信清单。"
