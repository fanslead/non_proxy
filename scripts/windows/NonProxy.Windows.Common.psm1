#requires -Version 5.1

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

function Test-NonProxyWindows {
    return [System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT
}

function Assert-NonProxyWindows {
    if (-not (Test-NonProxyWindows)) {
        throw "此操作只能在 Windows 主机运行。"
    }
}

function Test-NonProxyAdministrator {
    if (-not (Test-NonProxyWindows)) {
        return $false
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Assert-NonProxyAdministrator {
    Assert-NonProxyWindows
    if (-not (Test-NonProxyAdministrator)) {
        throw "此操作需要提升后的管理员 PowerShell。"
    }
}

function Assert-NonProxySystemMutation {
    param(
        [Parameter(Mandatory = $true)]
        [bool]$Confirmed
    )

    if (-not $Confirmed -or
        $env:NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION -ne "1") {
        throw (
            "系统变更被拒绝。必须同时传入 -ConfirmSystemMutation，" +
            "并设置 NONPROXY_ALLOW_WINDOWS_SYSTEM_MUTATION=1。")
    }
    Assert-NonProxyAdministrator
}

function ConvertTo-NonProxyThumbprint {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Thumbprint
    )

    $normalized = ($Thumbprint -replace "[^0-9A-Fa-f]", "").ToUpperInvariant()
    if ($normalized.Length -ne 40 -or
        $normalized -notmatch "^[0-9A-F]{40}$") {
        throw "发布者证书指纹必须是 40 位 SHA-1 十六进制文本。"
    }
    return $normalized
}

function Get-NonProxyFileSha256 {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-NonProxyCertificateSha256 {
    param(
        [Parameter(Mandatory = $true)]
        [Security.Cryptography.X509Certificates.X509Certificate2]$Certificate
    )

    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = $algorithm.ComputeHash($Certificate.RawData)
        return ([BitConverter]::ToString($hash) -replace "-", "").ToLowerInvariant()
    } finally {
        $algorithm.Dispose()
    }
}

function Resolve-NonProxyExistingPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [ValidateSet("Leaf", "Container")]
        [string]$PathType = "Leaf"
    )

    if (-not (Test-Path -LiteralPath $Path -PathType $PathType)) {
        throw "路径不存在或类型不正确：$Path"
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Resolve-NonProxyPackagePath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PackageRoot,
        [Parameter(Mandatory = $true)]
        [string]$RelativePath,
        [switch]$RequireExisting
    )

    if ([IO.Path]::IsPathRooted($RelativePath) -or
        $RelativePath.Contains("..") -or
        $RelativePath.Contains(":") -or
        $RelativePath.StartsWith("\") -or
        $RelativePath.StartsWith("/")) {
        throw "发布包相对路径无效：$RelativePath"
    }
    $root = [IO.Path]::GetFullPath($PackageRoot)
    if ($root -eq [IO.Path]::GetPathRoot($root)) {
        throw "发布包根目录不能是磁盘根目录。"
    }
    $root = $root.TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar)
    $candidate = [IO.Path]::GetFullPath(
        (Join-Path $root ($RelativePath -replace "/", [IO.Path]::DirectorySeparatorChar)))
    $prefix = $root + [IO.Path]::DirectorySeparatorChar
    if (-not $candidate.StartsWith(
        $prefix,
        [StringComparison]::OrdinalIgnoreCase)) {
        throw "发布包路径逃逸：$RelativePath"
    }
    if ($RequireExisting -and
        -not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        throw "发布包文件不存在：$RelativePath"
    }
    return $candidate
}

function Get-NonProxySignerThumbprint {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid -or
        $null -eq $signature.SignerCertificate) {
        throw "Authenticode 签名无效：$Path（$($signature.Status)）"
    }
    return ConvertTo-NonProxyThumbprint $signature.SignerCertificate.Thumbprint
}

function Get-NonProxySignerCertificateSha256 {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid -or
        $null -eq $signature.SignerCertificate) {
        throw "Authenticode 签名无效：$Path（$($signature.Status)）"
    }
    return Get-NonProxyCertificateSha256 `
        -Certificate $signature.SignerCertificate
}

function Get-NonProxyPackageRelativePath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PackageRoot,
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $root = [IO.Path]::GetFullPath($PackageRoot)
    if ($root -eq [IO.Path]::GetPathRoot($root)) {
        throw "发布包根目录不能是磁盘根目录。"
    }
    $root = $root.TrimEnd("\")
    $resolved = [IO.Path]::GetFullPath($Path)
    $prefix = $root + "\"
    if (-not $resolved.StartsWith(
        $prefix,
        [StringComparison]::OrdinalIgnoreCase)) {
        throw "发布文件不属于包根目录：$Path"
    }
    return $resolved.Substring($prefix.Length).Replace("\", "/")
}

function Assert-NonProxyAuthenticodeSignature {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$ExpectedPublisherThumbprint,
        [string]$DevelopmentRootCertificatePath
    )

    $expected = ConvertTo-NonProxyThumbprint $ExpectedPublisherThumbprint
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    $allowUntrustedDevelopmentRoot =
        -not [string]::IsNullOrWhiteSpace($DevelopmentRootCertificatePath)
    if ($null -eq $signature.SignerCertificate -or
        ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid -and
            (-not $allowUntrustedDevelopmentRoot -or
                $signature.Status -ne
                    [Management.Automation.SignatureStatus]::UnknownError))) {
        throw "Authenticode 签名无效：$Path（$($signature.Status)）"
    }
    $actual = ConvertTo-NonProxyThumbprint `
        $signature.SignerCertificate.Thumbprint
    if ($actual -ne $expected) {
        throw "文件发布者与固定指纹不匹配：$Path"
    }
    if (-not $allowUntrustedDevelopmentRoot) {
        return
    }

    $rootPath = Resolve-NonProxyExistingPath `
        -Path $DevelopmentRootCertificatePath -PathType Leaf
    $root = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
        $rootPath)
    $chain = [Security.Cryptography.X509Certificates.X509Chain]::new()
    try {
        $chain.ChainPolicy.RevocationMode =
            [Security.Cryptography.X509Certificates.X509RevocationMode]::NoCheck
        $allowUnknownAuthority = (
            [Security.Cryptography.X509Certificates.X509VerificationFlags]::AllowUnknownCertificateAuthority)
        $chain.ChainPolicy.VerificationFlags = $allowUnknownAuthority
        [void]$chain.ChainPolicy.ExtraStore.Add($root)
        if (-not $chain.Build($signature.SignerCertificate)) {
            $statuses = ($chain.ChainStatus | ForEach-Object {
                $_.Status.ToString()
            }) -join ","
            throw "开发签名证书链无效：$Path（$statuses）"
        }
        $last = $chain.ChainElements[$chain.ChainElements.Count - 1].Certificate
        $actualRoot = ConvertTo-NonProxyThumbprint $last.Thumbprint
        $expectedRoot = ConvertTo-NonProxyThumbprint $root.Thumbprint
        if ($actualRoot -ne $expectedRoot) {
            throw "开发签名证书链没有终止于固定根证书：$Path"
        }
    } finally {
        $chain.Dispose()
        $root.Dispose()
    }
}

function Get-NonProxySigningCertificate {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Thumbprint
    )

    $normalized = ConvertTo-NonProxyThumbprint $Thumbprint
    foreach ($storeName in @("Cert:\CurrentUser\My", "Cert:\LocalMachine\My")) {
        $certificate = Get-ChildItem -LiteralPath $storeName |
            Where-Object {
                $_.Thumbprint -eq $normalized -and $_.HasPrivateKey
            } |
            Select-Object -First 1
        if ($null -ne $certificate) {
            return $certificate
        }
    }
    throw "找不到带私钥的代码签名证书：$normalized"
}

function Invoke-NonProxyExternal {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [string[]]$Arguments = @(),
        [switch]$AllowFailure
    )

    & $FilePath @Arguments
    $exitCode = $LASTEXITCODE
    if (-not $AllowFailure -and $exitCode -ne 0) {
        throw "外部命令失败（$exitCode）：$FilePath"
    }
    return $exitCode
}

function Find-NonProxySignTool {
    Assert-NonProxyWindows
    $roots = @(
        (Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"),
        (Join-Path $env:ProgramFiles "Windows Kits\10\bin")
    )
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root -PathType Container)) {
            continue
        }
        $tool = Get-ChildItem -LiteralPath $root -Filter signtool.exe -Recurse |
            Where-Object { $_.FullName -match "\\x64\\signtool\.exe$" } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($null -ne $tool) {
            return $tool.FullName
        }
    }
    throw "找不到 Windows SDK signtool.exe。"
}

function Get-NonProxyServiceSnapshot {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    $service = Get-Service -Name $Name -ErrorAction SilentlyContinue
    if ($null -eq $service) {
        return [ordered]@{
            name = $Name
            installed = $false
            status = "Absent"
            startType = $null
        }
    }
    return [ordered]@{
        name = $Name
        installed = $true
        status = $service.Status.ToString()
        startType = $service.StartType.ToString()
    }
}

Export-ModuleMember -Function @(
    "Assert-NonProxyAdministrator",
    "Assert-NonProxyAuthenticodeSignature",
    "Assert-NonProxySystemMutation",
    "Assert-NonProxyWindows",
    "ConvertTo-NonProxyThumbprint",
    "Find-NonProxySignTool",
    "Get-NonProxyCertificateSha256",
    "Get-NonProxyFileSha256",
    "Get-NonProxyPackageRelativePath",
    "Get-NonProxyServiceSnapshot",
    "Get-NonProxySignerCertificateSha256",
    "Get-NonProxySignerThumbprint",
    "Get-NonProxySigningCertificate",
    "Invoke-NonProxyExternal",
    "Resolve-NonProxyExistingPath",
    "Resolve-NonProxyPackagePath",
    "Test-NonProxyAdministrator",
    "Test-NonProxyWindows"
)
