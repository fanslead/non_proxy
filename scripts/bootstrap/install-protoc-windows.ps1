#requires -Version 7.4

[CmdletBinding()]
param(
    [string]$InstallRoot
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$versionsPath = Join-Path $repositoryRoot "tools/versions.env"

if (-not $IsWindows) {
    throw "Protobuf Compiler Windows 安装器只能在 Windows 主机运行。"
}

$versions = @{}
foreach ($line in Get-Content -LiteralPath $versionsPath) {
    if ($line -match "^([A-Z0-9_]+)=(.+)$") {
        $versions[$Matches[1]] = $Matches[2]
    }
}
$version = [string]$versions.PROTOC_VERSION
$expectedSha256 = [string]$versions.PROTOC_WINDOWS_X64_SHA256
if ($version -notmatch "^[0-9]+\.[0-9]+(?:\.[0-9]+)?$" -or
    $expectedSha256 -notmatch "^[0-9a-f]{64}$") {
    throw "tools/versions.env 缺少有效的 Windows protoc 版本或 SHA-256。"
}

if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
    $InstallRoot = Join-Path $repositoryRoot ".tools/protoc-$version"
}
$installDirectory = [IO.Path]::GetFullPath($InstallRoot)
$toolsRoot = [IO.Path]::GetFullPath((Join-Path $repositoryRoot ".tools"))
if (-not $installDirectory.StartsWith(
    $toolsRoot + [IO.Path]::DirectorySeparatorChar,
    [StringComparison]::OrdinalIgnoreCase)) {
    throw "protoc 安装目录必须位于仓库 .tools 下。"
}

$protoc = Join-Path $installDirectory "bin/protoc.exe"
if (Test-Path -LiteralPath $protoc -PathType Leaf) {
    $installedVersion = (& $protoc --version).Trim()
    if ($LASTEXITCODE -eq 0 -and $installedVersion -eq "libprotoc $version") {
        if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_PATH)) {
            (Split-Path -Parent $protoc) |
                Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
        }
        Write-Host "已存在固定版本 protoc：$protoc"
        return
    }
}

$downloadDirectory = Join-Path $repositoryRoot ".tools/downloads"
$archiveName = "protoc-$version-win64.zip"
$archive = Join-Path $downloadDirectory $archiveName
$staging = Join-Path $repositoryRoot (
    ".tools/.protoc-staging-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $downloadDirectory, $staging |
    Out-Null

try {
    $uri = (
        "https://github.com/protocolbuffers/protobuf/releases/download/" +
        "v$version/$archiveName")
    Invoke-WebRequest -Uri $uri -OutFile $archive
    $actualSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).
        Hash.ToLowerInvariant()
    if ($actualSha256 -ne $expectedSha256) {
        throw "protoc Windows 压缩包 SHA-256 不匹配。"
    }
    Expand-Archive -LiteralPath $archive -DestinationPath $staging
    $stagedProtoc = Join-Path $staging "bin/protoc.exe"
    if (-not (Test-Path -LiteralPath $stagedProtoc -PathType Leaf)) {
        throw "protoc Windows 压缩包缺少 bin/protoc.exe。"
    }
    if (Test-Path -LiteralPath $installDirectory) {
        Remove-Item -LiteralPath $installDirectory -Recurse -Force
    }
    Move-Item -LiteralPath $staging -Destination $installDirectory
} finally {
    if (Test-Path -LiteralPath $staging) {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
}

$installedVersion = (& $protoc --version).Trim()
if ($LASTEXITCODE -ne 0 -or $installedVersion -ne "libprotoc $version") {
    throw "固定版本 protoc 安装后验证失败。"
}
if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_PATH)) {
    (Split-Path -Parent $protoc) |
        Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
}
Write-Host "固定版本 protoc 已安装：$protoc"
