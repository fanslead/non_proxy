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

New-Item -ItemType Directory -Force -Path $output, $intermediate | Out-Null
& $msbuild $project `
    /m `
    /t:Build `
    /p:Configuration=$Configuration `
    /p:Platform=$Platform `
    /p:SignMode=Off `
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
