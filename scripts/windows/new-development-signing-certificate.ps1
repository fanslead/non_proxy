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
    [string]$OutputDirectory
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

Assert-NonProxyWindows
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) {
    $existing = Get-ChildItem -LiteralPath $output -Force |
        Select-Object -First 1
    if ($null -ne $existing) {
        throw "开发证书输出目录不是空目录，拒绝覆盖：$output"
    }
}
New-Item -ItemType Directory -Force -Path $output | Out-Null

$subject = "CN=NonProxy Development $Version $Architecture"
Write-Host "正在生成临时自签名代码签名证书：$subject"
$certificate = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject $subject `
    -FriendlyName "NonProxy $Version $Architecture Development Signing" `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -KeyAlgorithm RSA `
    -KeyLength 3072 `
    -HashAlgorithm SHA256 `
    -KeyExportPolicy NonExportable `
    -NotAfter ([DateTimeOffset]::UtcNow.AddDays(90).UtcDateTime)
if ($null -eq $certificate -or -not $certificate.HasPrivateKey) {
    throw "Windows 开发代码签名证书创建失败。"
}
Write-Host "临时开发签名证书已写入当前用户个人证书存储。"

$certificatePath = Join-Path $output "NonProxy-Development.cer"
Export-Certificate -Cert $certificate -FilePath $certificatePath -Type CERT |
    Out-Null
Write-Host "临时开发签名公钥证书已导出。"
foreach ($store in @(
    "Cert:\CurrentUser\Root",
    "Cert:\CurrentUser\TrustedPublisher"
)) {
    Write-Host "正在将临时开发证书加入当前用户信任存储：$store"
    Import-Certificate -FilePath $certificatePath -CertStoreLocation $store |
        Out-Null
}
Write-Host "临时开发证书已加入当前用户信任存储。"

$certificateSha256 = Get-NonProxyCertificateSha256 -Certificate $certificate
[pscustomobject]@{
    Certificate = $certificate
    CertificatePath = $certificatePath
    CertificateSha256 = $certificateSha256
    Subject = $subject
    Thumbprint = ConvertTo-NonProxyThumbprint $certificate.Thumbprint
}
