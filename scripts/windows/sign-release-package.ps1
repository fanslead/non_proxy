#requires -Version 7.4

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageRoot,
    [Parameter(Mandatory = $true)]
    [string]$CertificateThumbprint,
    [Parameter(Mandatory = $true)]
    [ValidatePattern("^https://")]
    [string]$TimestampServer
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

Assert-NonProxyWindows
$root = Resolve-NonProxyExistingPath -Path $PackageRoot -PathType Container
$thumbprint = ConvertTo-NonProxyThumbprint $CertificateThumbprint
$certificate = Get-NonProxySigningCertificate -Thumbprint $thumbprint
$signTool = Find-NonProxySignTool
$manifestPath = Join-Path $root "release-manifest.json"
$trustPath = Join-Path $root "release-trust.ps1"

foreach ($generated in @($manifestPath, $trustPath)) {
    if (Test-Path -LiteralPath $generated) {
        throw "发布目录已包含签名输出，拒绝原地重签：$generated"
    }
}

$scriptExtensions = @(".ps1", ".psm1", ".psd1")
$allFiles = Get-ChildItem -LiteralPath $root -File -Recurse |
    Sort-Object FullName

foreach ($file in $allFiles) {
    $extension = $file.Extension.ToLowerInvariant()
    $relative = [IO.Path]::GetRelativePath(
        $root,
        $file.FullName).Replace("\", "/")
    $isProductBinary = $relative -in @(
        "desktop/NonProxy.Desktop.Windows.exe",
        "service/nonproxy-gatewayd.exe",
        "adapter/nonproxy-adapter-host.exe"
    ) -or $extension -in @(".msi", ".msix")
    if ($scriptExtensions -contains $extension) {
        $signature = Set-AuthenticodeSignature `
            -LiteralPath $file.FullName `
            -Certificate $certificate `
            -HashAlgorithm SHA256 `
            -TimestampServer $TimestampServer
        if ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid) {
            throw "PowerShell 工具签名失败：$($file.FullName)"
        }
    } elseif ($isProductBinary) {
        Invoke-NonProxyExternal -FilePath $signTool -Arguments @(
            "sign",
            "/sha1", $thumbprint,
            "/fd", "SHA256",
            "/tr", $TimestampServer,
            "/td", "SHA256",
            $file.FullName
        ) | Out-Null
    }
}

$driverCatalog = Join-Path $root "driver/NonProxyWfp.cat"
foreach ($driverFile in @(
    (Join-Path $root "driver/NonProxyWfp.inf"),
    (Join-Path $root "driver/NonProxyWfp.sys")
)) {
    Invoke-NonProxyExternal -FilePath $signTool -Arguments @(
        "verify", "/kp", "/c", $driverCatalog, $driverFile
    ) | Out-Null
}

$entries = foreach ($file in (
    Get-ChildItem -LiteralPath $root -File -Recurse | Sort-Object FullName)) {
    $relative = [IO.Path]::GetRelativePath($root, $file.FullName).Replace("\", "/")
    [ordered]@{
        path = $relative
        size = $file.Length
        sha256 = Get-NonProxyFileSha256 -Path $file.FullName
    }
}
$metadataPath = Join-Path $root "release-metadata.json"
$metadata = Get-Content -LiteralPath $metadataPath -Raw |
    ConvertFrom-Json
$manifest = [ordered]@{
    schemaVersion = 1
    product = $metadata.product
    version = $metadata.version
    architecture = $metadata.architecture
    minimumWindowsBuild = $metadata.minimumWindowsBuild
    publisherThumbprintHint = $thumbprint
    signedUtc = [DateTime]::UtcNow.ToString("o")
    files = @($entries)
}
$manifest | ConvertTo-Json -Depth 6 |
    Set-Content -LiteralPath $manifestPath -Encoding UTF8
$manifestHash = Get-NonProxyFileSha256 -Path $manifestPath

Set-Content -LiteralPath $trustPath -Encoding UTF8 -Value (
    "`$NonProxyReleaseManifestSha256 = '$manifestHash'")
$trustSignature = Set-AuthenticodeSignature `
    -LiteralPath $trustPath `
    -Certificate $certificate `
    -HashAlgorithm SHA256 `
    -TimestampServer $TimestampServer
if ($trustSignature.Status -ne [Management.Automation.SignatureStatus]::Valid) {
    throw "发布清单信任文件签名失败。"
}

Write-Host "Windows 发布目录签名完成：$root"
Write-Host "固定发布者指纹：$thumbprint"
Write-Warning (
    "企业代码签名不等于 Windows 生产内核信任。" +
    "驱动仍须经过 Microsoft Hardware Dev Center 的 Attestation/HLK 签名。")
