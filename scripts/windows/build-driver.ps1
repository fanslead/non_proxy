#requires -Version 7.4

[CmdletBinding()]
param(
    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64",
    [ValidateSet("Release")]
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$project = Join-Path $repositoryRoot "platform/windows/driver/NonProxyWfp.vcxproj"
$packagesConfig = Join-Path $repositoryRoot "platform/windows/driver/packages.config"
$wdkProps = Join-Path $repositoryRoot "platform/windows/driver/Directory.Build.props"
$versionsPath = Join-Path $repositoryRoot "tools/versions.env"
$packagesDirectory = Join-Path $repositoryRoot ".tools/windows-wdk"
$output = Join-Path $repositoryRoot ".artifacts/windows-driver/$Platform"
$intermediate = Join-Path $repositoryRoot ".artifacts/windows-driver/obj/$Platform"
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio/Installer/vswhere.exe"

if (-not $IsWindows) {
    throw "WFP Driver 必须在安装 WDK 的 Windows 主机上构建。"
}
if (-not (Test-Path $vswhere)) {
    throw "找不到 Visual Studio Installer 的 vswhere.exe。"
}

$msbuild = & $vswhere -latest -products * -requires Microsoft.Component.MSBuild `
    -find "MSBuild\**\Bin\MSBuild.exe" | Select-Object -First 1
if (-not $msbuild) {
    throw "找不到 MSBuild。"
}

$nuget = Get-Command nuget.exe -ErrorAction SilentlyContinue
if ($null -eq $nuget) {
    throw "找不到 nuget.exe，无法还原固定版本 WDK NuGet 包。"
}
$wdkVersionLine = Get-Content -LiteralPath $versionsPath |
    Where-Object { $_ -match "^WDK_NUGET_VERSION=" } |
    Select-Object -First 1
$wdkVersion = ($wdkVersionLine -replace "^WDK_NUGET_VERSION=", "").Trim()
$packageText = Get-Content -LiteralPath $packagesConfig -Raw
$propsText = Get-Content -LiteralPath $wdkProps -Raw
if ($wdkVersion -notmatch "^[0-9]+(?:\.[0-9]+){3}$" -or
    -not $packageText.Contains("version=`"$wdkVersion`"") -or
    -not $propsText.Contains(
        "<NonProxyWdkNuGetVersion>$wdkVersion</NonProxyWdkNuGetVersion>")) {
    throw "WDK NuGet 版本未与 tools/versions.env 同步。"
}

New-Item -ItemType Directory -Force -Path $packagesDirectory | Out-Null
& $nuget.Source restore $packagesConfig `
    -PackagesDirectory $packagesDirectory `
    -NonInteractive
if ($LASTEXITCODE -ne 0) {
    throw "固定版本 WDK NuGet 包还原失败。"
}

New-Item -ItemType Directory -Force -Path $output, $intermediate | Out-Null
& $msbuild $project `
    /m `
    /t:Build `
    /p:Configuration=$Configuration `
    /p:Platform=$Platform `
    /p:SignMode=Off `
    /p:NonProxyWdkPackagesDirectory="$packagesDirectory" `
    /p:OutDir="$output\" `
    /p:IntDir="$intermediate\" `
    /warnaserror
if ($LASTEXITCODE -ne 0) {
    throw "NonProxyWfp $Platform 构建失败。"
}

$driver = Get-ChildItem -Path $output -Filter "NonProxyWfp.sys" -Recurse |
    Select-Object -First 1
$inf = Get-ChildItem -Path $output -Filter "NonProxyWfp.inf" -Recurse |
    Select-Object -First 1
$catalog = Get-ChildItem -Path $output -Filter "NonProxyWfp.cat" -Recurse |
    Select-Object -First 1
if (-not $driver -or -not $inf -or -not $catalog) {
    throw "WDK 构建未生成完整的 SYS/INF/CAT 产物。"
}

Write-Host "WFP Driver 构建完成：$($driver.FullName)"
Write-Host "WFP INF 构建完成：$($inf.FullName)"
Write-Host "WFP Catalog 构建完成：$($catalog.FullName)"
