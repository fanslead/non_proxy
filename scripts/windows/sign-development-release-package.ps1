#requires -Version 7.4

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageRoot,
    [Parameter(Mandatory = $true)]
    [string]$CertificateThumbprint,
    [Parameter(Mandatory = $true)]
    [string]$PublicCertificatePath
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

Assert-NonProxyWindows
$root = Resolve-NonProxyExistingPath -Path $PackageRoot -PathType Container
$publicCertificate = Resolve-NonProxyExistingPath `
    -Path $PublicCertificatePath -PathType Leaf
$thumbprint = ConvertTo-NonProxyThumbprint $CertificateThumbprint
$certificate = Get-NonProxySigningCertificate -Thumbprint $thumbprint
$publisherCertificateSha256 = Get-NonProxyCertificateSha256 `
    -Certificate $certificate
$signTool = Find-NonProxySignTool
$manifestPath = Join-Path $root "release-manifest.json"
$trustPath = Join-Path $root "release-trust.ps1"
$metadataPath = Join-Path $root "release-metadata.json"
$metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
if ([string]$metadata.publisherCertificateSha256 -ne
    $publisherCertificateSha256) {
    throw "开发证书与发布产物编译固定的证书 SHA-256 不一致。"
}
foreach ($generated in @($manifestPath, $trustPath)) {
    if (Test-Path -LiteralPath $generated) {
        throw "发布目录已包含签名输出，拒绝原地重签：$generated"
    }
}

$developmentDirectory = Join-Path $root "development"
New-Item -ItemType Directory -Force -Path $developmentDirectory | Out-Null
Copy-Item -LiteralPath $publicCertificate `
    -Destination (Join-Path $developmentDirectory "NonProxy-Development.cer")
Set-Content `
    -LiteralPath (Join-Path $developmentDirectory "certificate-sha256.txt") `
    -Encoding UTF8 `
    -Value $publisherCertificateSha256
Copy-Item `
    -LiteralPath (Join-Path $PSScriptRoot "Install-Development-Certificate.ps1") `
    -Destination $developmentDirectory
$developmentNotice = @"
NonProxy $($metadata.version) Windows 开发预览版

此包使用临时自签名开发证书，不是面向普通用户的生产签名包。
驱动没有 Microsoft Hardware Dev Center 签名，只能在隔离测试机或虚拟机中使用。

使用前：
1. 以管理员 PowerShell 进入 development 目录。
2. 执行：
   .\Install-Development-Certificate.ps1 -ConfirmDevelopmentCertificateTrust -EnableTestSigning -ConfirmTestSigning
3. 重启 Windows。
4. 返回包根目录，运行 desktop\NonProxy.Desktop.Windows.exe。

启用了 Secure Boot 的机器通常不能开启 Test Signing。不要为日常主力机降低启动安全设置。
测试结束后可用同一脚本的 -Action Remove 移除开发证书；关闭 Test Signing 需另行执行
bcdedit.exe /set testsigning off 并重启。
"@
Set-Content `
    -LiteralPath (Join-Path $developmentDirectory "README.txt") `
    -Encoding UTF8 `
    -Value $developmentNotice

$metadata | Add-Member -NotePropertyName channel -NotePropertyValue "development"
$metadata | Add-Member `
    -NotePropertyName developmentSigned `
    -NotePropertyValue $true
$metadata | Add-Member `
    -NotePropertyName requiresWindowsTestSigning `
    -NotePropertyValue $true
$metadata | ConvertTo-Json -Depth 4 |
    Set-Content -LiteralPath $metadataPath -Encoding UTF8

$driverCatalog = Join-Path $root "driver/NonProxyWfp.cat"
Invoke-NonProxyExternal -FilePath $signTool -Arguments @(
    "sign", "/sha1", $thumbprint, "/fd", "SHA256", $driverCatalog
) | Out-Null

$scriptExtensions = @(".ps1", ".psm1", ".psd1")
$publisherSignedPaths = @(
    "desktop/NonProxy.Desktop.Windows.exe",
    "service/nonproxy-gatewayd.exe",
    "adapter/nonproxy-adapter-host.exe",
    "bootstrap/NonProxy.Windows.Bootstrap.exe"
)
$allFiles = Get-ChildItem -LiteralPath $root -File -Recurse |
    Sort-Object FullName
foreach ($file in $allFiles) {
    $relative = [IO.Path]::GetRelativePath(
        $root,
        $file.FullName).Replace("\", "/")
    if ($scriptExtensions -contains $file.Extension.ToLowerInvariant()) {
        $signature = Set-AuthenticodeSignature `
            -LiteralPath $file.FullName `
            -Certificate $certificate `
            -HashAlgorithm SHA256
        if ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid) {
            throw "PowerShell 开发签名失败：$relative（$($signature.Status)）"
        }
    } elseif ($publisherSignedPaths -contains $relative) {
        Invoke-NonProxyExternal -FilePath $signTool -Arguments @(
            "sign", "/sha1", $thumbprint, "/fd", "SHA256", $file.FullName
        ) | Out-Null
    }
}

foreach ($driverFile in @(
    (Join-Path $root "driver/NonProxyWfp.inf"),
    (Join-Path $root "driver/NonProxyWfp.sys")
)) {
    Invoke-NonProxyExternal -FilePath $signTool -Arguments @(
        "verify", "/pa", "/c", $driverCatalog, $driverFile
    ) | Out-Null
}
foreach ($relative in $publisherSignedPaths) {
    Invoke-NonProxyExternal -FilePath $signTool -Arguments @(
        "verify", "/pa", "/all", (Join-Path $root $relative)
    ) | Out-Null
}

$entries = foreach ($file in (
    Get-ChildItem -LiteralPath $root -File -Recurse | Sort-Object FullName)) {
    $relative = [IO.Path]::GetRelativePath($root, $file.FullName).
        Replace("\", "/")
    [ordered]@{
        path = $relative
        size = $file.Length
        sha256 = Get-NonProxyFileSha256 -Path $file.FullName
    }
}
$manifest = [ordered]@{
    schemaVersion = 1
    product = $metadata.product
    version = $metadata.version
    architecture = $metadata.architecture
    minimumWindowsBuild = $metadata.minimumWindowsBuild
    publisherCertificateSha256 = $publisherCertificateSha256
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
    -HashAlgorithm SHA256
if ($trustSignature.Status -ne [Management.Automation.SignatureStatus]::Valid) {
    throw "开发发布清单信任文件签名失败。"
}

[void](& (Join-Path $PSScriptRoot "verify-release-package.ps1") `
    -PackageRoot $root `
    -ExpectedPublisherThumbprint $thumbprint `
    -ConsumerBootstrapManifestSha256 $manifestHash `
    -PassThru)
Write-Host "Windows 开发预览发布目录签名完成：$root"
Write-Warning (
    "此包需要显式信任自签名证书并启用 Windows Test Signing；" +
    "它不具备生产内核信任。")
