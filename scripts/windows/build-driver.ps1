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

$vsInstallPath = $null
foreach ($wdkComponent in @(
    "Microsoft.Windows.DriverKit",
    "Component.Microsoft.Windows.DriverKit.BuildTools"
)) {
    $candidate = & $vswhere -latest -products * `
        -requires $wdkComponent `
        -property installationPath | Select-Object -First 1
    if (-not [string]::IsNullOrWhiteSpace($candidate)) {
        $vsInstallPath = $candidate.Trim()
        break
    }
}
if ([string]::IsNullOrWhiteSpace($vsInstallPath)) {
    throw "找不到包含 Windows Driver Kit 组件的 Visual Studio。"
}
$devShellModule = Join-Path $vsInstallPath (
    "Common7/Tools/Microsoft.VisualStudio.DevShell.dll")
if (-not (Test-Path -LiteralPath $devShellModule -PathType Leaf)) {
    throw "找不到 Visual Studio Developer Shell 模块。"
}
Import-Module $devShellModule -Force
Enter-VsDevShell -VsInstallPath $vsInstallPath
Set-Location $repositoryRoot
$msbuild = (Get-Command msbuild.exe -ErrorAction Stop).Source
if ([string]::IsNullOrWhiteSpace($msbuild)) {
    throw "Visual Studio Developer Shell 未提供 MSBuild。"
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

$wdkPackageRoot = Join-Path $packagesDirectory (
    "Microsoft.Windows.WDK.x64.$wdkVersion")
$wdkHostToolDirectories = @(
    Get-ChildItem -LiteralPath $wdkPackageRoot -Filter *.exe -File -Recurse |
        Where-Object { $_.Directory.Name -eq "x64" } |
        ForEach-Object { $_.Directory.FullName }
    Get-ChildItem -LiteralPath $wdkPackageRoot -Filter *.exe -File -Recurse |
        Where-Object { $_.Directory.Name -eq "x86" } |
        ForEach-Object { $_.Directory.FullName }
) | Select-Object -Unique
if ($wdkHostToolDirectories.Count -eq 0) {
    throw "固定版本 WDK NuGet 包缺少 Windows 主机工具。"
}
$env:Path = (($wdkHostToolDirectories -join ";") + ";" + $env:Path)
foreach ($requiredTool in @("stampinf.exe", "Inf2Cat.exe")) {
    if ($null -eq (Get-Command $requiredTool -ErrorAction SilentlyContinue)) {
        throw "固定版本 WDK NuGet 包未提供 $requiredTool。"
    }
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

$driverPackageDirectory = Join-Path $output "NonProxyWfp"
foreach ($driverFileName in @(
    "NonProxyWfp.inf",
    "NonProxyWfp.sys",
    "NonProxyWfp.cat"
)) {
    $packagedDriverFile = Join-Path $driverPackageDirectory $driverFileName
    if (-not (Test-Path -LiteralPath $packagedDriverFile -PathType Leaf)) {
        throw "WDK Driver Package 缺少 $driverFileName。"
    }
    Copy-Item -LiteralPath $packagedDriverFile `
        -Destination (Join-Path $output $driverFileName) `
        -Force
}

Write-Host "WFP Driver Package 构建完成：$output"
