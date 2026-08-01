#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageRoot,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedPublisherThumbprint,
    [string]$ConsumerBootstrapManifestSha256,
    [switch]$PassThru
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

Assert-NonProxyWindows
$root = Resolve-NonProxyExistingPath -Path $PackageRoot -PathType Container
$expected = ConvertTo-NonProxyThumbprint $ExpectedPublisherThumbprint
$manifestPath = Join-Path $root "release-manifest.json"
$trustPath = Join-Path $root "release-trust.ps1"
$manifestPath = Resolve-NonProxyExistingPath -Path $manifestPath -PathType Leaf
$trustPath = Resolve-NonProxyExistingPath -Path $trustPath -PathType Leaf

Assert-NonProxyAuthenticodeSignature `
    -Path $trustPath `
    -ExpectedPublisherThumbprint $expected
$trustText = Get-Content -LiteralPath $trustPath -Raw
$signatureMarker = "# SIG # Begin signature block"
$markerIndex = $trustText.IndexOf($signatureMarker, [StringComparison]::Ordinal)
if ($markerIndex -lt 0) {
    throw "发布信任文件缺少 Authenticode 签名块。"
}
$unsignedTrustText = $trustText.Substring(0, $markerIndex).Trim()
$match = [regex]::Match(
    $unsignedTrustText,
    "^\`$NonProxyReleaseManifestSha256 = '([0-9a-f]{64})'$")
if (-not $match.Success) {
    throw "发布信任文件内容不符合固定格式。"
}
$manifestHash = Get-NonProxyFileSha256 -Path $manifestPath
if ($manifestHash -ne $match.Groups[1].Value) {
    throw "发布清单哈希与已签名信任文件不匹配。"
}
if (-not [string]::IsNullOrWhiteSpace($ConsumerBootstrapManifestSha256) -and
    ($ConsumerBootstrapManifestSha256 -notmatch "^[0-9a-f]{64}$" -or
        $ConsumerBootstrapManifestSha256 -ne $manifestHash)) {
    throw "消费安装 Bootstrap 验证的发布清单哈希不匹配。"
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.schemaVersion -ne 1 -or $manifest.product -ne "NonProxy") {
    throw "发布清单版本或产品标识不受支持。"
}
$publisherCertificateSha256 = Get-NonProxySignerCertificateSha256 `
    -Path $trustPath
if ([string]$manifest.publisherCertificateSha256 -notmatch "^[0-9a-f]{64}$" -or
    [string]$manifest.publisherCertificateSha256 -ne
        $publisherCertificateSha256) {
    throw "发布清单固定证书 SHA-256 与签名者不一致。"
}
if ([string]$manifest.publisherThumbprintHint -ne $expected) {
    throw "发布清单证书指纹提示与外部固定发布者不一致。"
}
if ($manifest.architecture -notin @("x64", "arm64")) {
    throw "发布清单架构无效。"
}
if ($manifest.minimumWindowsBuild -lt 18362) {
    throw "发布清单最低 Windows Build 无效。"
}
$currentBuild = [Environment]::OSVersion.Version.Build
if ($currentBuild -lt [int]$manifest.minimumWindowsBuild) {
    throw "当前 Windows Build 低于发布包最低要求。"
}
$nativeArchitecture = if (
    $env:PROCESSOR_ARCHITEW6432 -eq "ARM64" -or
    $env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
    "arm64"
} elseif (
    $env:PROCESSOR_ARCHITEW6432 -eq "AMD64" -or
    $env:PROCESSOR_ARCHITECTURE -eq "AMD64") {
    "x64"
} else {
    throw "不支持当前 Windows 原生架构。"
}
if ($manifest.architecture -ne $nativeArchitecture) {
    throw "发布包架构与当前 Windows 原生架构不匹配。"
}
if ($null -eq $manifest.files -or $manifest.files.Count -eq 0) {
    throw "发布清单没有文件。"
}

$expectedFiles = [Collections.Generic.HashSet[string]]::new(
    [StringComparer]::OrdinalIgnoreCase)
$scriptExtensions = @(".ps1", ".psm1", ".psd1")
$publisherSignedPaths = @(
    "desktop/NonProxy.Desktop.Windows.exe",
    "service/nonproxy-gatewayd.exe",
    "adapter/nonproxy-adapter-host.exe",
    "bootstrap/NonProxy.Windows.Bootstrap.exe"
)
foreach ($entry in $manifest.files) {
    $relative = [string]$entry.path
    if (-not $expectedFiles.Add($relative)) {
        throw "发布清单包含重复路径：$relative"
    }
    $path = Resolve-NonProxyPackagePath `
        -PackageRoot $root `
        -RelativePath $relative `
        -RequireExisting
    $file = Get-Item -LiteralPath $path -Force
    if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "发布包不允许重解析点：$relative"
    }
    if ($file.Length -ne [long]$entry.size) {
        throw "发布包文件大小不匹配：$relative"
    }
    $actualHash = Get-NonProxyFileSha256 -Path $path
    if ($actualHash -ne [string]$entry.sha256) {
        throw "发布包文件哈希不匹配：$relative"
    }
    if ($scriptExtensions -contains $file.Extension.ToLowerInvariant() -or
        $publisherSignedPaths -contains $relative -or
        $file.Extension.ToLowerInvariant() -in @(".msi", ".msix")) {
        Assert-NonProxyAuthenticodeSignature `
            -Path $path `
            -ExpectedPublisherThumbprint $expected
    }
}
foreach ($requiredPublisherFile in $publisherSignedPaths) {
    if (-not $expectedFiles.Contains($requiredPublisherFile)) {
        throw "发布包缺少必须由固定发布者签名的入口：$requiredPublisherFile"
    }
}

foreach ($actual in (
    Get-ChildItem -LiteralPath $root -File -Recurse | Sort-Object FullName)) {
    if ($actual.FullName -in @($manifestPath, $trustPath)) {
        continue
    }
    $relative = Get-NonProxyPackageRelativePath `
        -PackageRoot $root `
        -Path $actual.FullName
    if (-not $expectedFiles.Contains($relative)) {
        throw "发布包包含清单外文件：$relative"
    }
}

if ([string]::IsNullOrWhiteSpace($ConsumerBootstrapManifestSha256)) {
    $signTool = Find-NonProxySignTool
    $driverCatalog = Join-Path $root "driver/NonProxyWfp.cat"
    foreach ($driverFile in @(
        (Join-Path $root "driver/NonProxyWfp.inf"),
        (Join-Path $root "driver/NonProxyWfp.sys")
    )) {
        Invoke-NonProxyExternal -FilePath $signTool -Arguments @(
            "verify", "/kp", "/c", $driverCatalog, $driverFile
        ) | Out-Null
    }
}

$result = [ordered]@{
    trusted = $true
    product = [string]$manifest.product
    version = [string]$manifest.version
    architecture = [string]$manifest.architecture
    minimumWindowsBuild = [int]$manifest.minimumWindowsBuild
    publisherThumbprint = $expected
    publisherCertificateSha256 = $publisherCertificateSha256
    packageRoot = $root
    manifestSha256 = $manifestHash
}
if ($PassThru) {
    return [pscustomobject]$result
}
$result | ConvertTo-Json -Compress
