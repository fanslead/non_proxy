#requires -Version 7.4

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet("Query", "Enable", "Reset")]
    [string]$Action,
    [switch]$ConfirmSystemMutation,
    [switch]$AcknowledgeTestMachineOnly,
    [switch]$ConfirmResetAllVerifierSettings
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

Assert-NonProxyWindows
$verifier = Join-Path $env:SystemRoot "System32\verifier.exe"
if (-not (Test-Path -LiteralPath $verifier -PathType Leaf)) {
    throw "找不到 verifier.exe。"
}

if ($Action -eq "Query") {
    & $verifier /querysettings
    $queryExitCode = $LASTEXITCODE
    exit $queryExitCode
}

Assert-NonProxySystemMutation -Confirmed $ConfirmSystemMutation.IsPresent
if (-not $AcknowledgeTestMachineOnly) {
    throw "Driver Verifier 只能用于可恢复的测试机，必须显式确认。"
}

if ($Action -eq "Enable") {
    Invoke-NonProxyExternal -FilePath $verifier `
        -Arguments @("/standard", "/driver", "NonProxyWfp.sys") | Out-Null
    Write-Warning "Verifier 设置已写入；脚本不会自动重启。"
    exit 0
}

if (-not $ConfirmResetAllVerifierSettings -or
    $env:NONPROXY_CONFIRM_VERIFIER_GLOBAL_RESET -ne "1") {
    throw (
        "verifier /reset 会清除整台机器的 Verifier 设置。" +
        "必须传入 -ConfirmResetAllVerifierSettings 并设置 " +
        "NONPROXY_CONFIRM_VERIFIER_GLOBAL_RESET=1。")
}
Invoke-NonProxyExternal -FilePath $verifier -Arguments @("/reset") | Out-Null
Write-Warning "Verifier 全局设置已清除；脚本不会自动重启。"
