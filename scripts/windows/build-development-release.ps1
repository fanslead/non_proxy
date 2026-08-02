#requires -Version 7.4

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern("^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$")]
    [string]$Version,
    [Parameter(Mandatory = $true)]
    [ValidateSet("x64", "arm64")]
    [string]$Architecture
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

Assert-NonProxyWindows
$versionSource = Get-Content `
    -LiteralPath (Join-Path $repositoryRoot "Directory.Build.props") `
    -Raw
$versionMatch = [regex]::Match(
    $versionSource,
    "<Version>([^<]+)</Version>")
if (-not $versionMatch.Success -or $versionMatch.Groups[1].Value -ne $Version) {
    throw "发布版本与 Directory.Build.props 中的仓库版本不一致。"
}
$target = if ($Architecture -eq "x64") {
    [pscustomobject]@{
        DriverPlatform = "x64"
        Rid = "win-x64"
        RustTarget = "x86_64-pc-windows-msvc"
    }
} else {
    [pscustomobject]@{
        DriverPlatform = "ARM64"
        Rid = "win-arm64"
        RustTarget = "aarch64-pc-windows-msvc"
    }
}

$certificateDirectory = Join-Path $repositoryRoot (
    ".artifacts/windows-development-certificate/$Version/$Architecture")
$signing = & (Join-Path $PSScriptRoot "new-development-signing-certificate.ps1") `
    -Version $Version `
    -Architecture $Architecture `
    -OutputDirectory $certificateDirectory

$desktopOutput = Join-Path $repositoryRoot (
    ".artifacts/desktop/$($target.Rid)")
$bootstrapOutput = Join-Path $repositoryRoot (
    ".artifacts/bootstrap/$($target.Rid)")
& dotnet publish (
    Join-Path $repositoryRoot (
        "apps/desktop/NonProxy.Desktop.Windows/" +
        "NonProxy.Desktop.Windows.csproj")) `
    -c Release `
    -f net10.0-windows10.0.26100.0 `
    -r $target.Rid `
    --self-contained true `
    "-p:NonProxyWindowsPublisherCertificateSha256=$($signing.CertificateSha256)" `
    -o $desktopOutput
if ($LASTEXITCODE -ne 0) {
    throw "Windows 桌面端发布失败。"
}
& dotnet publish (
    Join-Path $repositoryRoot (
        "platform/windows/NonProxy.Windows.Bootstrap/" +
        "NonProxy.Windows.Bootstrap.csproj")) `
    -c Release `
    -f net10.0-windows10.0.26100.0 `
    -r $target.Rid `
    --self-contained true `
    -p:PublishSingleFile=true `
    -p:IncludeNativeLibrariesForSelfExtract=true `
    "-p:NonProxyWindowsPublisherCertificateSha256=$($signing.CertificateSha256)" `
    -o $bootstrapOutput
if ($LASTEXITCODE -ne 0) {
    throw "Windows 安装 Bootstrap 发布失败。"
}

& cargo +1.97.1 build --locked --release `
    --target $target.RustTarget `
    -p nonproxy-gatewayd `
    -p nonproxy-adapter-host
if ($LASTEXITCODE -ne 0) {
    throw "Windows Rust 服务发布失败。"
}
& (Join-Path $PSScriptRoot "build-driver.ps1") `
    -Platform $target.DriverPlatform

$packageName = "NonProxy-$Version-windows-$Architecture-development"
$packageRoot = Join-Path $repositoryRoot (
    ".artifacts/windows-development/$Version/$packageName")
& (Join-Path $PSScriptRoot "build-release-package.ps1") `
    -Version $Version `
    -Architecture $Architecture `
    -DesktopPublishDirectory $desktopOutput `
    -BootstrapPublishDirectory $bootstrapOutput `
    -ExpectedPublisherCertificateSha256 $signing.CertificateSha256 `
    -GatewayExecutable (Join-Path $repositoryRoot (
        "target/$($target.RustTarget)/release/nonproxy-gatewayd.exe")) `
    -AdapterHostExecutable (Join-Path $repositoryRoot (
        "target/$($target.RustTarget)/release/nonproxy-adapter-host.exe")) `
    -DriverDirectory (Join-Path $repositoryRoot (
        ".artifacts/windows-driver/$($target.DriverPlatform)")) `
    -OutputDirectory $packageRoot
& (Join-Path $PSScriptRoot "sign-development-release-package.ps1") `
    -PackageRoot $packageRoot `
    -CertificateThumbprint $signing.Thumbprint `
    -PublicCertificatePath $signing.CertificatePath

$releaseDirectory = Join-Path $repositoryRoot ".artifacts/release/$Version"
New-Item -ItemType Directory -Force -Path $releaseDirectory | Out-Null
$archive = Join-Path $releaseDirectory "$packageName.zip"
if (Test-Path -LiteralPath $archive) {
    throw "开发预览版压缩包已存在，拒绝覆盖：$archive"
}
Compress-Archive -LiteralPath $packageRoot -DestinationPath $archive
$archiveSha256 = Get-NonProxyFileSha256 -Path $archive
Set-Content `
    -LiteralPath "$archive.sha256" `
    -Encoding UTF8 `
    -Value "$archiveSha256  $packageName.zip"

Write-Host "Windows 开发预览版已生成：$archive"
if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_OUTPUT)) {
    "archive=$archive" |
        Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    "sha256=$archiveSha256" |
        Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
}
