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

$certificatePath = Join-Path $output "NonProxy-Development.cer"
Export-Certificate -Cert $certificate -FilePath $certificatePath -Type CERT |
    Out-Null
foreach ($store in @(
    "Cert:\CurrentUser\Root",
    "Cert:\CurrentUser\TrustedPublisher"
)) {
    Import-Certificate -FilePath $certificatePath -CertStoreLocation $store |
        Out-Null
}

$certificateSha256 = Get-NonProxyCertificateSha256 -Certificate $certificate
[pscustomobject]@{
    Certificate = $certificate
    CertificatePath = $certificatePath
    CertificateSha256 = $certificateSha256
    Subject = $subject
    Thumbprint = ConvertTo-NonProxyThumbprint $certificate.Thumbprint
}
