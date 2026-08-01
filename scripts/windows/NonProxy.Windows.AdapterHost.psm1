#requires -Version 7.4

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
Import-Module (
    Join-Path $PSScriptRoot "NonProxy.Windows.Common.psm1") -Force

function Resolve-NonProxyAdapterExecutablePath {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout,
        [Parameter(Mandatory = $true)]
        [string]$Executable
    )

    $resolved = Resolve-NonProxyExistingPath -Path $Executable -PathType Leaf
    $programRoot = [IO.Path]::GetFullPath($Layout.ProgramRoot).TrimEnd("\")
    $adapterDirectory = [IO.Path]::GetDirectoryName($resolved)
    $versionRoot = [IO.Path]::GetDirectoryName($adapterDirectory)
    $versionParent = [IO.Path]::GetDirectoryName($versionRoot)
    if (-not $versionParent.Equals(
            $programRoot,
            [StringComparison]::OrdinalIgnoreCase) -or
        [IO.Path]::GetFileName($adapterDirectory) -ne "adapter" -or
        [IO.Path]::GetFileName($resolved) -ne "nonproxy-adapter-host.exe") {
        throw "Adapter Host 必须位于受保护的 NonProxy 版本目录。"
    }
    foreach ($trustedPath in @(
        $programRoot,
        $versionRoot,
        $adapterDirectory,
        $resolved
    )) {
        $item = Get-Item -LiteralPath $trustedPath -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Adapter Host 信任路径不允许使用重解析点。"
        }
    }
    return $resolved
}

function Assert-NonProxyAdapterExecutable {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout,
        [Parameter(Mandatory = $true)]
        [string]$Executable,
        [Parameter(Mandatory = $true)]
        [string]$Fingerprint
    )

    $resolved = Resolve-NonProxyAdapterExecutablePath `
        -Layout $Layout `
        -Executable $Executable
    if ($Fingerprint -notmatch "^[0-9a-f]{64}$" -or
        (Get-NonProxyFileSha256 -Path $resolved) -ne $Fingerprint) {
        throw "Adapter Host 指纹与已验证发布包不一致。"
    }
    return $resolved
}

function Get-NonProxyAdapterHostTaskSnapshot {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout,
        [AllowNull()]
        [string]$ExpectedExecutable,
        [AllowNull()]
        [string]$ExpectedFingerprint
    )

    $task = Get-ScheduledTask `
        -TaskName $Layout.AdapterTaskName `
        -TaskPath $Layout.AdapterTaskPath `
        -ErrorAction SilentlyContinue
    if ($null -eq $task) {
        return [ordered]@{
            installed = $false
            status = "Absent"
            executable = $null
            definitionValid = $false
        }
    }
    $actions = @($task.Actions)
    $triggers = @($task.Triggers)
    $actualExecutable = if ($actions.Count -eq 1) {
        [string]$actions[0].Execute
    } else {
        $null
    }
    $expectedResolved = $null
    $definitionValid = $false
    try {
        $expectedResolved = if (
            [string]::IsNullOrWhiteSpace($ExpectedExecutable)) {
            $null
        } else {
            [IO.Path]::GetFullPath($ExpectedExecutable)
        }
        $definitionValid = (
            $actions.Count -eq 1 -and
            [string]::IsNullOrWhiteSpace([string]$actions[0].Arguments) -and
            -not [string]::IsNullOrWhiteSpace($actualExecutable) -and
            -not [string]::IsNullOrWhiteSpace($expectedResolved) -and
            [IO.Path]::GetFullPath($actualExecutable).Equals(
                $expectedResolved,
                [StringComparison]::OrdinalIgnoreCase) -and
            [IO.Path]::GetFullPath([string]$actions[0].WorkingDirectory).Equals(
                [IO.Path]::GetDirectoryName($expectedResolved),
                [StringComparison]::OrdinalIgnoreCase) -and
            [string]$task.Principal.GroupId -eq $Layout.AdapterUsersGroupSid -and
            [string]$task.Principal.LogonType -eq "Group" -and
            [string]$task.Principal.RunLevel -eq "Limited" -and
            $triggers.Count -eq 1 -and
            [string]$triggers[0].CimClass.CimClassName -eq "MSFT_TaskLogonTrigger" -and
            [string]::IsNullOrWhiteSpace([string]$triggers[0].UserId) -and
            [string]$task.Settings.MultipleInstances -eq "Parallel")
    } catch {
        $definitionValid = $false
    }
    if ($definitionValid) {
        try {
            [void](Assert-NonProxyAdapterExecutable `
                -Layout $Layout `
                -Executable $actualExecutable `
                -Fingerprint $ExpectedFingerprint)
        } catch {
            $definitionValid = $false
        }
    }
    return [ordered]@{
        installed = $true
        status = [string]$task.State
        executable = $actualExecutable
        definitionValid = $definitionValid
    }
}

function Set-NonProxyAdapterHostTask {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout,
        [Parameter(Mandatory = $true)]
        [string]$Executable,
        [Parameter(Mandatory = $true)]
        [string]$Fingerprint
    )

    $resolved = Assert-NonProxyAdapterExecutable `
        -Layout $Layout `
        -Executable $Executable `
        -Fingerprint $Fingerprint
    $action = New-ScheduledTaskAction `
        -Execute $resolved `
        -WorkingDirectory ([IO.Path]::GetDirectoryName($resolved))
    $trigger = New-ScheduledTaskTrigger -AtLogOn
    $principal = New-ScheduledTaskPrincipal `
        -GroupId $Layout.AdapterUsersGroupSid `
        -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet `
        -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries `
        -StartWhenAvailable `
        -MultipleInstances Parallel `
        -RestartCount 3 `
        -RestartInterval (New-TimeSpan -Minutes 1) `
        -ExecutionTimeLimit ([TimeSpan]::Zero) `
        -Compatibility Win8
    $definition = New-ScheduledTask `
        -Action $action `
        -Trigger $trigger `
        -Principal $principal `
        -Settings $settings `
        -Description "NonProxy 当前用户第三方客户端适配宿主"
    Register-ScheduledTask `
        -TaskName $Layout.AdapterTaskName `
        -TaskPath $Layout.AdapterTaskPath `
        -InputObject $definition `
        -Force | Out-Null
    $snapshot = Get-NonProxyAdapterHostTaskSnapshot `
        -Layout $Layout `
        -ExpectedExecutable $resolved `
        -ExpectedFingerprint $Fingerprint
    if (-not $snapshot.definitionValid) {
        throw "Adapter Host 登录任务登记后校验失败。"
    }
}

function Remove-NonProxyAdapterHostTask {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout
    )

    $task = Get-ScheduledTask `
        -TaskName $Layout.AdapterTaskName `
        -TaskPath $Layout.AdapterTaskPath `
        -ErrorAction SilentlyContinue
    if ($null -eq $task) {
        return
    }
    Stop-ScheduledTask `
        -TaskName $Layout.AdapterTaskName `
        -TaskPath $Layout.AdapterTaskPath `
        -ErrorAction SilentlyContinue
    Unregister-ScheduledTask `
        -TaskName $Layout.AdapterTaskName `
        -TaskPath $Layout.AdapterTaskPath `
        -Confirm:$false
}

function Stop-NonProxyAdapterHostProcesses {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$Layout,
        [AllowNull()]
        [string]$InstallRoot
    )

    $expectedExecutable = if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
        $null
    } else {
        Resolve-NonProxyAdapterExecutablePath `
            -Layout $Layout `
            -Executable (
                Join-Path $InstallRoot "adapter\nonproxy-adapter-host.exe")
    }
    $processes = @(Get-CimInstance Win32_Process -Filter `
        "Name = 'nonproxy-adapter-host.exe'")
    foreach ($process in $processes) {
        $executable = [string]$process.ExecutablePath
        if ([string]::IsNullOrWhiteSpace($executable)) {
            continue
        }
        try {
            $resolved = Resolve-NonProxyAdapterExecutablePath `
                -Layout $Layout `
                -Executable $executable
        } catch {
            continue
        }
        if ($null -ne $expectedExecutable -and
            -not $resolved.Equals(
                $expectedExecutable,
                [StringComparison]::OrdinalIgnoreCase)) {
            continue
        }
        $termination = Invoke-CimMethod `
            -InputObject $process `
            -MethodName Terminate
        if ([uint32]$termination.ReturnValue -ne 0) {
            throw "无法终止已安装的 Adapter Host 进程 $($process.ProcessId)。"
        }
    }
}

Export-ModuleMember -Function @(
    "Get-NonProxyAdapterHostTaskSnapshot",
    "Remove-NonProxyAdapterHostTask",
    "Set-NonProxyAdapterHostTask",
    "Stop-NonProxyAdapterHostProcesses"
)
