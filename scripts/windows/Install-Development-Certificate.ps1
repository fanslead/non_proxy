#requires -Version 5.1
#requires -RunAsAdministrator

[CmdletBinding()]
param(
    [ValidateSet("Install", "Remove")]
    [string]$Action = "Install",
    [switch]$ConfirmDevelopmentCertificateTrust,
    [switch]$EnableTestSigning,
    [switch]$ConfirmTestSigning
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

if (-not $ConfirmDevelopmentCertificateTrust) {
    throw (
        "此脚本会修改本机证书信任。请确认只在测试机使用后，" +
        "显式传入 -ConfirmDevelopmentCertificateTrust。")
}
if ($EnableTestSigning -xor $ConfirmTestSigning) {
    throw (
        "启用 Windows Test Signing 必须同时传入 " +
        "-EnableTestSigning 与 -ConfirmTestSigning。")
}

$certificatePath = Join-Path $PSScriptRoot "NonProxy-Development.cer"
$rootCertificatePath = Join-Path $PSScriptRoot "NonProxy-Development-Root.cer"
$sha256Path = Join-Path $PSScriptRoot "certificate-sha256.txt"
$rootSha256Path = Join-Path $PSScriptRoot "root-certificate-sha256.txt"
if (-not (Test-Path -LiteralPath $certificatePath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $rootCertificatePath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $sha256Path -PathType Leaf) -or
    -not (Test-Path -LiteralPath $rootSha256Path -PathType Leaf)) {
    throw "开发签名证书、根证书或固定 SHA-256 文件缺失。"
}
$expectedSha256 = (Get-Content -LiteralPath $sha256Path -Raw).Trim().ToLowerInvariant()
$actualSha256 = (Get-FileHash -LiteralPath $certificatePath -Algorithm SHA256).
    Hash.ToLowerInvariant()
if ($expectedSha256 -notmatch "^[0-9a-f]{64}$" -or
    $actualSha256 -ne $expectedSha256) {
    throw "开发签名证书 SHA-256 与发布包固定值不一致。"
}
$expectedRootSha256 = (
    Get-Content -LiteralPath $rootSha256Path -Raw).Trim().ToLowerInvariant()
$actualRootSha256 = (
    Get-FileHash -LiteralPath $rootCertificatePath -Algorithm SHA256).
        Hash.ToLowerInvariant()
if ($expectedRootSha256 -notmatch "^[0-9a-f]{64}$" -or
    $actualRootSha256 -ne $expectedRootSha256) {
    throw "开发根证书 SHA-256 与发布包固定值不一致。"
}
$certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
    $certificatePath)
$rootCertificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
    $rootCertificatePath)
$thumbprint = $certificate.Thumbprint.ToUpperInvariant()
$rootThumbprint = $rootCertificate.Thumbprint.ToUpperInvariant()

if ($Action -eq "Install") {
    foreach ($trust in @(
        [pscustomobject]@{
            Path = $rootCertificatePath
            Store = "Cert:\LocalMachine\Root"
        },
        [pscustomobject]@{
            Path = $certificatePath
            Store = "Cert:\LocalMachine\TrustedPublisher"
        }
    )) {
        Import-Certificate `
            -FilePath $trust.Path `
            -CertStoreLocation $trust.Store |
            Out-Null
    }
    Write-Host (
        "已信任 NonProxy 开发 CA 与签名证书：" +
        "$rootThumbprint / $thumbprint")
    if ($EnableTestSigning) {
        & bcdedit.exe /set testsigning on
        if ($LASTEXITCODE -ne 0) {
            throw (
                "无法启用 Test Signing。启用了 Secure Boot 的机器通常会拒绝此操作，" +
                "请改用隔离测试机或虚拟机。")
        }
        Write-Warning "Test Signing 已启用，重启 Windows 后生效。"
    }
    return
}

foreach ($storePath in @(
    "Cert:\LocalMachine\Root\$rootThumbprint",
    "Cert:\LocalMachine\TrustedPublisher\$thumbprint"
)) {
    if (Test-Path -LiteralPath $storePath) {
        Remove-Item -LiteralPath $storePath -Force
    }
}
Write-Host "已移除 NonProxy 开发证书：$thumbprint"
Write-Warning (
    "此操作不会自动关闭 Windows Test Signing。确认没有其他测试驱动依赖后，" +
    "可手动执行 bcdedit.exe /set testsigning off 并重启。")
