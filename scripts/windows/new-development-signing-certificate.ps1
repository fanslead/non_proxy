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
$rsa = [Security.Cryptography.RSA]::Create(3072)
$generatedCertificate = $null
$certificate = $null
$pfxBytes = $null
try {
    $request = [Security.Cryptography.X509Certificates.CertificateRequest]::new(
        $subject,
        $rsa,
        [Security.Cryptography.HashAlgorithmName]::SHA256,
        [Security.Cryptography.RSASignaturePadding]::Pkcs1)
    [void]$request.CertificateExtensions.Add(
        [Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new(
            $false, $false, 0, $true))
    [void]$request.CertificateExtensions.Add(
        [Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new(
            [Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature,
            $true))
    $enhancedKeyUsages = [Security.Cryptography.OidCollection]::new()
    [void]$enhancedKeyUsages.Add(
        [Security.Cryptography.Oid]::new(
            "1.3.6.1.5.5.7.3.3", "Code Signing"))
    [void]$request.CertificateExtensions.Add(
        [Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new(
            $enhancedKeyUsages, $true))
    [void]$request.CertificateExtensions.Add(
        [Security.Cryptography.X509Certificates.X509SubjectKeyIdentifierExtension]::new(
            $request.PublicKey, $false))
    $now = [DateTimeOffset]::UtcNow
    $generatedCertificate = $request.CreateSelfSigned(
        $now.AddMinutes(-5), $now.AddDays(90))
    $pfxPassword = [Guid]::NewGuid().ToString("N")
    $pfxBytes = $generatedCertificate.Export(
        [Security.Cryptography.X509Certificates.X509ContentType]::Pfx,
        $pfxPassword)
    $keyStorageFlags =
        [Security.Cryptography.X509Certificates.X509KeyStorageFlags]::UserKeySet `
        -bor [Security.Cryptography.X509Certificates.X509KeyStorageFlags]::PersistKeySet
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
        $pfxBytes, $pfxPassword, $keyStorageFlags)
    $certificate.FriendlyName =
        "NonProxy $Version $Architecture Development Signing"
    $personalStore = [Security.Cryptography.X509Certificates.X509Store]::new(
        [Security.Cryptography.X509Certificates.StoreName]::My,
        [Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser)
    try {
        $personalStore.Open(
            [Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
        $personalStore.Add($certificate)
    } finally {
        $personalStore.Dispose()
    }
} finally {
    if ($null -ne $pfxBytes) {
        [Array]::Clear($pfxBytes, 0, $pfxBytes.Length)
    }
    if ($null -ne $generatedCertificate) {
        $generatedCertificate.Dispose()
    }
    $rsa.Dispose()
}
if ($null -eq $certificate -or -not $certificate.HasPrivateKey) {
    throw "Windows 开发代码签名证书创建失败。"
}
Write-Host "临时开发签名证书已写入当前用户个人证书存储。"

$certificatePath = Join-Path $output "NonProxy-Development.cer"
$publicCertificateBytes = $certificate.Export(
    [Security.Cryptography.X509Certificates.X509ContentType]::Cert)
[IO.File]::WriteAllBytes($certificatePath, $publicCertificateBytes)
Write-Host "临时开发签名公钥证书已导出。"
$publicCertificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
    $publicCertificateBytes)
foreach ($storeName in @(
    [Security.Cryptography.X509Certificates.StoreName]::TrustedPeople,
    [Security.Cryptography.X509Certificates.StoreName]::TrustedPublisher
)) {
    Write-Host "正在将临时开发证书加入当前用户信任存储：$storeName"
    $trustStore = [Security.Cryptography.X509Certificates.X509Store]::new(
        $storeName,
        [Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser)
    try {
        $trustStore.Open(
            [Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
        $trustStore.Add($publicCertificate)
    } finally {
        $trustStore.Dispose()
    }
}
$publicCertificate.Dispose()
Write-Host "临时开发证书已加入当前用户信任存储。"

$certificateSha256 = Get-NonProxyCertificateSha256 -Certificate $certificate
[pscustomobject]@{
    Certificate = $certificate
    CertificatePath = $certificatePath
    CertificateSha256 = $certificateSha256
    Subject = $subject
    Thumbprint = ConvertTo-NonProxyThumbprint $certificate.Thumbprint
}
