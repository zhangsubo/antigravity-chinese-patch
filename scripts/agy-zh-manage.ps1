[CmdletBinding()]
param(
    [string]$InstallDir = "",
    [string]$From = "",
    [string]$Version = "",
    [string]$Mirror = "",
    [string]$App = "",
    [ValidateSet("", "cdp", "asar")]
    [string]$Mode = "",
    [switch]$Update,
    [switch]$Status,
    [switch]$Logs,
    [switch]$ClearLogs,
    [switch]$Restore,
    [switch]$Uninstall,
    [switch]$RestoreApp,
    [switch]$DeleteSource,
    [switch]$NoPath,
    [switch]$ReplaceStaleBackup,
    [switch]$Yes,
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

try {
    [Console]::InputEncoding = [System.Text.Encoding]::UTF8
    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
    $OutputEncoding = [System.Text.Encoding]::UTF8
} catch {}

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {}

$Script:ScriptVersion = "3.0.5"
$Script:Repo = if ([string]::IsNullOrWhiteSpace($env:AGY_ZH_REPO)) { "zhangsubo/antigravity-chinese-patch" } else { $env:AGY_ZH_REPO.Trim() }
$Script:ReleasesBaseUrl = if ([string]::IsNullOrWhiteSpace($env:AGY_ZH_RELEASES_BASE_URL)) { "https://github.com/$($Script:Repo)/releases" } else { $env:AGY_ZH_RELEASES_BASE_URL.Trim() }
$Script:ApiLatestUrl = if ([string]::IsNullOrWhiteSpace($env:AGY_ZH_API_LATEST_URL)) { "https://api.github.com/repos/$($Script:Repo)/releases/latest" } else { $env:AGY_ZH_API_LATEST_URL.Trim() }

$Script:IsPipedIex = [string]::IsNullOrEmpty($PSCommandPath)
$Script:ScriptPath = $PSCommandPath
$Script:ScriptDir = if ([string]::IsNullOrEmpty($PSCommandPath)) { "" } else { Split-Path -Parent $PSCommandPath }

$Script:LocalAppData = if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
    $env:LOCALAPPDATA
} elseif (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
    Join-Path $env:USERPROFILE "AppData\Local"
} else {
    [System.IO.Path]::GetTempPath()
}

$Script:ConfigDir = if (-not [string]::IsNullOrWhiteSpace($env:AGY_ZH_CONFIG_DIR)) {
    $env:AGY_ZH_CONFIG_DIR
} else {
    Join-Path $Script:LocalAppData "antigravity-chinese-patch"
}
$Script:ConfFile = Join-Path $Script:ConfigDir "manager.json"
$Script:LogFile = if (-not [string]::IsNullOrWhiteSpace($env:AGY_ZH_LOG_FILE)) {
    $env:AGY_ZH_LOG_FILE
} else {
    Join-Path $Script:LocalAppData "antigravity-chinese-patch\antigravity-chinese-patch.log"
}

function Get-AssetName {
    $arch = ""
    try {
        $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToUpperInvariant()
    } catch {
        $rawArch = $env:PROCESSOR_ARCHITEW6432
        if ([string]::IsNullOrWhiteSpace($rawArch)) {
            $rawArch = $env:PROCESSOR_ARCHITECTURE
        }
        if ($null -ne $rawArch) {
            $arch = $rawArch.ToUpperInvariant()
        }
    }
    if ($arch -eq "ARM64") {
        return "agy-zh-windows-arm64.exe"
    }
    return "agy-zh-windows-x64.exe"
}

$Script:AssetName = Get-AssetName
$Script:TempWorkDir = ""
$Script:LastSumsFile = ""
$Script:LastResolvedTag = ""
$Script:EffectiveInstallDir = ""
$Script:EffectiveMirror = ""
$Script:EffectiveApp = ""
$Script:TargetBin = ""
$Script:TargetScript = ""
$Script:TargetCmd = ""

function Show-Usage {
    Write-Host @"
Antigravity 简体中文汉化工具 (agy-zh) Windows 一键安装与维护脚本

用法:
  agy-zh-manage.cmd [选项]
  powershell -NoProfile -ExecutionPolicy Bypass -File agy-zh-manage.ps1 [选项]

主要动作:
  -Mode cdp                   一键部署零改包 CDP 动态汉化（推荐，生成桌面快捷方式）
  -Mode asar                  一键安装静态 ASAR 安全补丁
  -Update                     在线检查并更新 agy-zh 与本地维护脚本（自动刷新快捷方式）
  -Status                     查看本地/云端版本、安装目录、快捷方式与 doctor 诊断摘要
  -Logs                       查看最近 50 行 CDP 动态守护日志
  -ClearLogs                  清空 CDP 动态守护日志
  -Restore                    将 Antigravity 还原为官方原版（执行 agy-zh.exe restore）
  -Uninstall                  彻底卸载 agy-zh.exe、维护脚本、快捷方式与用户 PATH

常用选项:
  -InstallDir <DIR>           指定并持久化 agy-zh 安装目录（默认 %LOCALAPPDATA%\Programs\agy-zh）
  -From <FILE>                指定本地已下载的 agy-zh-*.exe 文件进行迁移并重命名
  -Version <TAG>              指定下载版本（如 v3.0.5，默认 latest）
  -Mirror <PREFIX|none>       配置并持久化 GitHub 下载加速前缀（传 none 清除）
  -App <PATH>                 指定并持久化 Antigravity 安装目录
  -DeleteSource               使用 -From 本地迁移成功后删除源文件
  -NoPath                     不修改用户环境变量 PATH
  -ReplaceStaleBackup         静态安装时自动清理旧版本残留备份
  -Yes                        非交互确认
  -Help                       显示帮助信息

不带动作参数运行时，将进入中文交互式维护菜单。
"@
}

function Cleanup-TempDir {
    if (-not [string]::IsNullOrEmpty($Script:TempWorkDir) -and (Test-Path -LiteralPath $Script:TempWorkDir)) {
        try { Remove-Item -LiteralPath $Script:TempWorkDir -Recurse -Force -ErrorAction SilentlyContinue } catch {}
    }
}

function Ensure-TempDir {
    if ([string]::IsNullOrEmpty($Script:TempWorkDir) -or -not (Test-Path -LiteralPath $Script:TempWorkDir)) {
        $name = "agy-zh-manage-" + [System.Guid]::NewGuid().ToString("N").Substring(0, 8)
        $Script:TempWorkDir = Join-Path ([System.IO.Path]::GetTempPath()) $name
        New-Item -ItemType Directory -Path $Script:TempWorkDir -Force | Out-Null
    }
}

function Test-IsInteractive {
    if ($Yes) {
        return $false
    }
    try {
        if ([Environment]::UserInteractive -and -not [Console]::IsInputRedirected) {
            return $true
        }
    } catch {}
    return $false
}

function Read-Prompt([string]$PromptText) {
    Write-Host -NoNewline $PromptText
    try {
        $line = [Console]::In.ReadLine()
        if ($null -eq $line) {
            return ""
        }
        return $line.Trim()
    } catch {
        return ""
    }
}

function Get-JsonProp($Obj, [string]$Name) {
    if ($null -eq $Obj) {
        return $null
    }
    $prop = $Obj.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    return $prop.Value
}

function Load-Config {
    $res = @{
        INSTALL_DIR = ""
        MIRROR = ""
        APP_PATH = ""
    }
    if (-not (Test-Path -LiteralPath $Script:ConfFile)) {
        return $res
    }
    try {
        $raw = Get-Content -LiteralPath $Script:ConfFile -Raw -Encoding UTF8
        if (-not [string]::IsNullOrWhiteSpace($raw)) {
            $parsed = $raw | ConvertFrom-Json
            $v1 = Get-JsonProp $parsed "INSTALL_DIR"
            $v2 = Get-JsonProp $parsed "MIRROR"
            $v3 = Get-JsonProp $parsed "APP_PATH"
            if ($null -ne $v1) { $res.INSTALL_DIR = [string]$v1 }
            if ($null -ne $v2) { $res.MIRROR = [string]$v2 }
            if ($null -ne $v3) { $res.APP_PATH = [string]$v3 }
        }
    } catch {}
    return $res
}

function Save-Config {
    if (-not (Test-Path -LiteralPath $Script:ConfigDir)) {
        New-Item -ItemType Directory -Path $Script:ConfigDir -Force | Out-Null
    }
    $obj = [ordered]@{
        INSTALL_DIR = $Script:EffectiveInstallDir
        MIRROR = $Script:EffectiveMirror
        APP_PATH = $Script:EffectiveApp
    }
    $json = $obj | ConvertTo-Json -Depth 3
    $tmp = Join-Path $Script:ConfigDir (".manager.json.tmp." + $PID)
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($tmp, $json, $utf8NoBom)
    Move-Item -LiteralPath $tmp -Destination $Script:ConfFile -Force
}

function Expand-UserPath([string]$RawPath) {
    if ([string]::IsNullOrWhiteSpace($RawPath)) {
        return ""
    }
    $expanded = [Environment]::ExpandEnvironmentVariables($RawPath.Trim())
    if ($expanded -eq "~") {
        return $env:USERPROFILE
    }
    if ($expanded.StartsWith("~/") -or $expanded.StartsWith("~\")) {
        return Join-Path $env:USERPROFILE $expanded.Substring(2)
    }
    return $expanded
}

function Resolve-EffectiveSettings {
    $conf = Load-Config

    if (-not [string]::IsNullOrWhiteSpace($InstallDir)) {
        $Script:EffectiveInstallDir = Expand-UserPath $InstallDir
    } elseif (-not [string]::IsNullOrWhiteSpace($env:AGY_ZH_INSTALL_DIR)) {
        $Script:EffectiveInstallDir = Expand-UserPath $env:AGY_ZH_INSTALL_DIR
    } elseif (-not [string]::IsNullOrWhiteSpace($conf.INSTALL_DIR)) {
        $Script:EffectiveInstallDir = Expand-UserPath $conf.INSTALL_DIR
    } else {
        $Script:EffectiveInstallDir = Join-Path $Script:LocalAppData "Programs\agy-zh"
    }

    if ($PSBoundParameters.ContainsKey("Mirror")) {
        $Script:EffectiveMirror = $Mirror.Trim()
    } elseif (Test-Path Env:AGY_ZH_MIRROR) {
        $Script:EffectiveMirror = $env:AGY_ZH_MIRROR.Trim()
    } else {
        $Script:EffectiveMirror = $conf.MIRROR.Trim()
    }
    if ($Script:EffectiveMirror.ToLowerInvariant() -eq "none") {
        $Script:EffectiveMirror = ""
    }
    $Script:EffectiveMirror = $Script:EffectiveMirror.TrimEnd("/")

    if (-not [string]::IsNullOrWhiteSpace($App)) {
        $Script:EffectiveApp = Expand-UserPath $App
    } elseif (-not [string]::IsNullOrWhiteSpace($env:AGY_ZH_APP)) {
        $Script:EffectiveApp = Expand-UserPath $env:AGY_ZH_APP
    } else {
        $Script:EffectiveApp = Expand-UserPath $conf.APP_PATH
    }

    $Script:TargetBin = Join-Path $Script:EffectiveInstallDir "agy-zh.exe"
    $Script:TargetScript = Join-Path $Script:EffectiveInstallDir "agy-zh-manage.ps1"
    $Script:TargetCmd = Join-Path $Script:EffectiveInstallDir "agy-zh-manage.cmd"
}

function Invoke-NativeCommand([string]$ExePath, [string[]]$CmdArgs) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $outLines = & $ExePath @CmdArgs 2>&1
        $code = $LASTEXITCODE
        $text = ""
        if ($null -ne $outLines) {
            $text = (($outLines | ForEach-Object { $_.ToString() }) -join "`n").Trim()
        }
        return @{
            ExitCode = $code
            Output = $text
        }
    } finally {
        $ErrorActionPreference = $prev
    }
}

function Apply-MirrorUrl([string]$RawUrl) {
    if ([string]::IsNullOrWhiteSpace($Script:EffectiveMirror)) {
        return $RawUrl
    }
    if ($RawUrl.StartsWith("file://", [System.StringComparison]::OrdinalIgnoreCase)) {
        return $RawUrl
    }
    if ($RawUrl.StartsWith("https://github.com/") -or $RawUrl.StartsWith("https://raw.githubusercontent.com/") -or $RawUrl.StartsWith("http://github.com/")) {
        return "$($Script:EffectiveMirror)/$RawUrl"
    }
    return $RawUrl
}

function Fetch-UrlToFile([string]$RawUrl, [string]$DestPath) {
    if ($RawUrl.StartsWith("file://", [System.StringComparison]::OrdinalIgnoreCase)) {
        $localPath = ([Uri]$RawUrl).LocalPath
        if (-not (Test-Path -LiteralPath $localPath)) {
            throw "本地测试资源不存在: $localPath"
        }
        Copy-Item -LiteralPath $localPath -Destination $DestPath -Force
        return
    }

    $targetUrl = Apply-MirrorUrl $RawUrl
    $wc = New-Object System.Net.WebClient
    try {
        $wc.Encoding = [System.Text.Encoding]::UTF8
        $wc.Headers.Add("User-Agent", "agy-zh-manage")
        $wc.DownloadFile($targetUrl, $DestPath)
    } catch {
        throw "下载失败: $targetUrl ($($_.Exception.Message))。网络提示: 若直连 GitHub 超时，可配置代理、使用 -Mirror <加速前缀>，或手动从 Releases 下载 $($Script:AssetName) 后用 -From 本地迁移。"
    } finally {
        $wc.Dispose()
    }
}

function Compute-Sha256([string]$FilePath) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $stream = [System.IO.File]::OpenRead($FilePath)
        try {
            $hashBytes = $sha.ComputeHash($stream)
            return ([System.BitConverter]::ToString($hashBytes).Replace("-", "")).ToLowerInvariant()
        } finally {
            $stream.Dispose()
        }
    } finally {
        $sha.Dispose()
    }
}

function Lookup-Sha256InSums([string]$SumsFile, [string]$Asset) {
    $lines = Get-Content -LiteralPath $SumsFile -Encoding UTF8
    foreach ($line in $lines) {
        if ($line -match '^\s*([0-9a-fA-F]{64})\s+[*]?(.+?)\s*$') {
            $hash = $Matches[1].ToLowerInvariant()
            $name = $Matches[2].Trim()
            if ($name.StartsWith("./") -or $name.StartsWith(".\")) {
                $name = $name.Substring(2)
            }
            if ($name -eq $Asset) {
                return $hash
            }
        }
    }
    return ""
}

function Verify-Sha256([string]$FilePath, [string]$SumsFile, [string]$Asset) {
    $expected = Lookup-Sha256InSums $SumsFile $Asset
    if ([string]::IsNullOrEmpty($expected)) {
        throw "SHA256SUMS.txt 中未找到 $Asset 的校验条目"
    }
    $actual = Compute-Sha256 $FilePath
    if ($actual -ne $expected) {
        throw "SHA-256 校验失败: $Asset`n  期望值: $expected`n  实际值: $actual"
    }
}

function Normalize-Tag([string]$RawTag) {
    $t = $RawTag.Trim()
    if ([string]::IsNullOrEmpty($t) -or $t -eq "latest") {
        return ""
    }
    if ($t.StartsWith("v")) {
        return $t
    }
    return "v$t"
}

function Resolve-LatestTag {
    if (-not [string]::IsNullOrWhiteSpace($Version)) {
        return (Normalize-Tag $Version)
    }

    if ($Script:ReleasesBaseUrl.StartsWith("file://", [System.StringComparison]::OrdinalIgnoreCase)) {
        $baseLocal = ([Uri]$Script:ReleasesBaseUrl).LocalPath
        $latestFile = Join-Path $baseLocal "LATEST"
        if (-not (Test-Path -LiteralPath $latestFile)) {
            throw "本地测试目录缺少 LATEST 文件: $latestFile"
        }
        $firstLine = (Get-Content -LiteralPath $latestFile -Encoding UTF8 | Select-Object -First 1)
        return (Normalize-Tag ([string]$firstLine))
    }

    $tag = ""
    $latestUrl = Apply-MirrorUrl ($Script:ReleasesBaseUrl.TrimEnd("/") + "/latest")
    try {
        $req = [System.Net.HttpWebRequest]::Create($latestUrl)
        $req.Method = "HEAD"
        $req.AllowAutoRedirect = $true
        $req.Timeout = 15000
        $req.UserAgent = "agy-zh-manage"
        $resp = $req.GetResponse()
        try {
            $finalUri = $resp.ResponseUri.AbsoluteUri
            if ($finalUri -match '/tag/([^/?#]+)') {
                $tag = $Matches[1]
            }
        } finally {
            $resp.Close()
        }
    } catch {}

    if ([string]::IsNullOrEmpty($tag)) {
        $wc = New-Object System.Net.WebClient
        try {
            $wc.Encoding = [System.Text.Encoding]::UTF8
            $wc.Headers.Add("User-Agent", "agy-zh-manage")
            $apiUrl = Apply-MirrorUrl $Script:ApiLatestUrl
            $apiJson = $wc.DownloadString($apiUrl)
            $apiResp = $apiJson | ConvertFrom-Json
            $tagName = Get-JsonProp $apiResp "tag_name"
            if ($null -ne $tagName) {
                $tag = [string]$tagName
            }
        } catch {} finally {
            $wc.Dispose()
        }
    }

    if ([string]::IsNullOrEmpty($tag)) {
        throw "无法获取最新 Release 版本号。请检查网络、配置 -Mirror，或通过 -Version / -From 安装。"
    }
    return (Normalize-Tag $tag)
}

function Get-LocalVersion {
    if (Test-Path -LiteralPath $Script:TargetBin) {
        $res = Invoke-NativeCommand $Script:TargetBin @("--version")
        if ($res.ExitCode -eq 0 -and -not [string]::IsNullOrWhiteSpace($res.Output)) {
            $first = ($res.Output -split "`n")[0].Trim()
            $parts = $first -split "\s+"
            return $parts[$parts.Length - 1]
        }
    }
    return ""
}

function Discover-LocalAsset {
    $cwdCandidate = Join-Path (Get-Location).Path $Script:AssetName
    if (Test-Path -LiteralPath $cwdCandidate -PathType Leaf) {
        return $cwdCandidate
    }
    if (-not [string]::IsNullOrEmpty($Script:ScriptDir)) {
        $dirCandidate = Join-Path $Script:ScriptDir $Script:AssetName
        if (Test-Path -LiteralPath $dirCandidate -PathType Leaf) {
            return $dirCandidate
        }
    }
    return ""
}

function Normalize-PathEntry([string]$Entry) {
    return $Entry.Trim().TrimEnd("\").ToLowerInvariant()
}

function Ensure-UserPathConfigured {
    if ($NoPath -or $env:AGY_ZH_SKIP_USER_PATH -eq "1") {
        return
    }
    $targetNorm = Normalize-PathEntry $Script:EffectiveInstallDir
    $currentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($null -eq $currentUserPath) {
        $currentUserPath = ""
    }
    $parts = @()
    $exists = $false
    foreach ($p in ($currentUserPath -split ";")) {
        if ([string]::IsNullOrWhiteSpace($p)) {
            continue
        }
        $parts += $p
        if ((Normalize-PathEntry $p) -eq $targetNorm) {
            $exists = $true
        }
    }
    if (-not $exists) {
        $newParts = @($Script:EffectiveInstallDir) + $parts
        $newPath = $newParts -join ";"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "已将 $($Script:EffectiveInstallDir) 添加至用户 PATH（新开终端生效）"
    }

    $procExists = $false
    foreach ($p in ($env:Path -split ";")) {
        if (-not [string]::IsNullOrWhiteSpace($p) -and (Normalize-PathEntry $p) -eq $targetNorm) {
            $procExists = $true
            break
        }
    }
    if (-not $procExists) {
        $env:Path = "$($Script:EffectiveInstallDir);$($env:Path)"
    }
}

function Remove-UserPathEntry {
    if ($NoPath -or $env:AGY_ZH_SKIP_USER_PATH -eq "1") {
        return
    }
    $targetNorm = Normalize-PathEntry $Script:EffectiveInstallDir
    $currentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ([string]::IsNullOrWhiteSpace($currentUserPath)) {
        return
    }
    $kept = @()
    $changed = $false
    foreach ($p in ($currentUserPath -split ";")) {
        if ([string]::IsNullOrWhiteSpace($p)) {
            continue
        }
        if ((Normalize-PathEntry $p) -eq $targetNorm) {
            $changed = $true
        } else {
            $kept += $p
        }
    }
    if ($changed) {
        [Environment]::SetEnvironmentVariable("Path", ($kept -join ";"), "User")
        Write-Host "已从用户 PATH 移除 $($Script:EffectiveInstallDir)"
    }
}

function Install-BinaryFromStaged([string]$StagedSrc) {
    if (-not (Test-Path -LiteralPath $Script:EffectiveInstallDir)) {
        New-Item -ItemType Directory -Path $Script:EffectiveInstallDir -Force | Out-Null
    }
    $tmpBin = Join-Path $Script:EffectiveInstallDir (".agy-zh.tmp." + $PID + ".exe")
    Copy-Item -LiteralPath $StagedSrc -Destination $tmpBin -Force
    try { Unblock-File -LiteralPath $tmpBin -ErrorAction SilentlyContinue } catch {}

    $verCheck = Invoke-NativeCommand $tmpBin @("--version")
    if ($verCheck.ExitCode -ne 0) {
        Remove-Item -LiteralPath $tmpBin -Force -ErrorAction SilentlyContinue
        throw "待安装的可执行文件自检失败（无法执行 --version），已中止以避免覆盖现有版本"
    }

    if (Test-Path -LiteralPath $Script:TargetBin) {
        $null = Invoke-NativeCommand $Script:TargetBin @("stop")
    }

    Move-Item -LiteralPath $tmpBin -Destination $Script:TargetBin -Force
    $verText = ($verCheck.Output -split "`n")[0].Trim()
    Write-Host "已安装 agy-zh ($verText) -> $($Script:TargetBin)"
}

function Install-FromLocalFile([string]$SrcFile) {
    if (-not (Test-Path -LiteralPath $SrcFile -PathType Leaf)) {
        throw "找不到本地文件: $SrcFile"
    }
    $resolvedSrc = (Resolve-Path -LiteralPath $SrcFile).Path
    $srcDir = Split-Path -Parent $resolvedSrc
    $srcBase = Split-Path -Leaf $resolvedSrc
    $sumsCandidate = Join-Path $srcDir "SHA256SUMS.txt"
    if (Test-Path -LiteralPath $sumsCandidate -PathType Leaf) {
        $expected = Lookup-Sha256InSums $sumsCandidate $srcBase
        if (-not [string]::IsNullOrEmpty($expected)) {
            Write-Host "检测到同目录 SHA256SUMS.txt，正在校验 $srcBase..."
            Verify-Sha256 $resolvedSrc $sumsCandidate $srcBase
            Write-Host "SHA-256 校验通过。"
        }
    }

    Install-BinaryFromStaged $resolvedSrc
    Save-Config

    $dstNorm = [System.IO.Path]::GetFullPath($Script:TargetBin).ToLowerInvariant()
    $srcNorm = [System.IO.Path]::GetFullPath($resolvedSrc).ToLowerInvariant()
    if ($srcNorm -ne $dstNorm) {
        if ($DeleteSource) {
            Remove-Item -LiteralPath $resolvedSrc -Force
            Write-Host "已清理源文件: $resolvedSrc"
        } elseif (Test-IsInteractive) {
            $delAns = Read-Prompt "是否删除原下载文件 $resolvedSrc？[y/N]: "
            if ($delAns.ToLowerInvariant() -eq "y" -or $delAns.ToLowerInvariant() -eq "yes") {
                Remove-Item -LiteralPath $resolvedSrc -Force
                Write-Host "已删除源文件: $resolvedSrc"
            }
        }
    }
}

function Install-FromRelease {
    Ensure-TempDir
    $tag = Resolve-LatestTag
    $Script:LastResolvedTag = $tag
    $baseDl = $Script:ReleasesBaseUrl.TrimEnd("/") + "/download/" + $tag
    $sumsTmp = Join-Path $Script:TempWorkDir "SHA256SUMS.txt"
    $binTmp = Join-Path $Script:TempWorkDir $Script:AssetName

    Write-Host "正在从 Release $tag 下载校验清单与 $($Script:AssetName)..."
    Fetch-UrlToFile "$baseDl/SHA256SUMS.txt" $sumsTmp
    Fetch-UrlToFile "$baseDl/$($Script:AssetName)" $binTmp
    Verify-Sha256 $binTmp $sumsTmp $Script:AssetName
    Write-Host "SHA-256 校验通过。"

    $Script:LastSumsFile = $sumsTmp
    Install-BinaryFromStaged $binTmp
    Save-Config
}

function Write-DefaultCmdWrapper([string]$CmdDest) {
    $cmdContent = "@echo off`r`nsetlocal`r`nset `"PSModulePath=`"`r`nset `"PS_SCRIPT=%~dp0agy-zh-manage.ps1`"`r`nif not exist `"%PS_SCRIPT%`" (`r`n  echo [ERROR] Missing agy-zh-manage.ps1 next to agy-zh-manage.cmd`r`n  if `"%~1`"==`"`" pause`r`n  exit /b 1`r`n)`r`npowershell.exe -NoProfile -ExecutionPolicy Bypass -File `"%PS_SCRIPT%`" %*`r`nset `"EXIT_CODE=%ERRORLEVEL%`"`r`nif `"%~1`"==`"`" (`r`n  echo.`r`n  pause`r`n)`r`nexit /b %EXIT_CODE%`r`n"
    $ascii = New-Object System.Text.ASCIIEncoding
    [System.IO.File]::WriteAllText($CmdDest, $cmdContent, $ascii)
}

function Sync-ManageScripts {
    if (-not (Test-Path -LiteralPath $Script:EffectiveInstallDir)) {
        New-Item -ItemType Directory -Path $Script:EffectiveInstallDir -Force | Out-Null
    }

    $syncedPs1 = $false
    $syncedCmd = $false

    if (-not [string]::IsNullOrEmpty($Script:LastSumsFile) -and (Test-Path -LiteralPath $Script:LastSumsFile)) {
        $baseDl = $Script:ReleasesBaseUrl.TrimEnd("/") + "/download/" + $Script:LastResolvedTag
        $ps1Expected = Lookup-Sha256InSums $Script:LastSumsFile "agy-zh-manage.ps1"
        if (-not [string]::IsNullOrEmpty($ps1Expected)) {
            $curHash = ""
            if (Test-Path -LiteralPath $Script:TargetScript) {
                $curHash = Compute-Sha256 $Script:TargetScript
            }
            if ($curHash -ne $ps1Expected) {
                Ensure-TempDir
                $ps1Tmp = Join-Path $Script:TempWorkDir "agy-zh-manage.ps1"
                Fetch-UrlToFile "$baseDl/agy-zh-manage.ps1" $ps1Tmp
                Verify-Sha256 $ps1Tmp $Script:LastSumsFile "agy-zh-manage.ps1"
                Copy-Item -LiteralPath $ps1Tmp -Destination $Script:TargetScript -Force
                try { Unblock-File -LiteralPath $Script:TargetScript -ErrorAction SilentlyContinue } catch {}
                Write-Host "已同步维护脚本 -> $($Script:TargetScript)"
            }
            $syncedPs1 = $true
        }

        $cmdExpected = Lookup-Sha256InSums $Script:LastSumsFile "agy-zh-manage.cmd"
        if (-not [string]::IsNullOrEmpty($cmdExpected)) {
            $curCmdHash = ""
            if (Test-Path -LiteralPath $Script:TargetCmd) {
                $curCmdHash = Compute-Sha256 $Script:TargetCmd
            }
            if ($curCmdHash -ne $cmdExpected) {
                Ensure-TempDir
                $cmdTmp = Join-Path $Script:TempWorkDir "agy-zh-manage.cmd"
                Fetch-UrlToFile "$baseDl/agy-zh-manage.cmd" $cmdTmp
                Verify-Sha256 $cmdTmp $Script:LastSumsFile "agy-zh-manage.cmd"
                Copy-Item -LiteralPath $cmdTmp -Destination $Script:TargetCmd -Force
                try { Unblock-File -LiteralPath $Script:TargetCmd -ErrorAction SilentlyContinue } catch {}
            }
            $syncedCmd = $true
        }
    }

    if (-not $syncedPs1 -and -not [string]::IsNullOrEmpty($Script:ScriptPath) -and (Test-Path -LiteralPath $Script:ScriptPath)) {
        $srcNorm = [System.IO.Path]::GetFullPath($Script:ScriptPath).ToLowerInvariant()
        $dstNorm = [System.IO.Path]::GetFullPath($Script:TargetScript).ToLowerInvariant()
        if ($srcNorm -ne $dstNorm) {
            Copy-Item -LiteralPath $Script:ScriptPath -Destination $Script:TargetScript -Force
            try { Unblock-File -LiteralPath $Script:TargetScript -ErrorAction SilentlyContinue } catch {}
            Write-Host "已安装维护脚本 -> $($Script:TargetScript)"
        }
        $syncedPs1 = $true
    }

    if (-not $syncedCmd) {
        $localCmd = ""
        if (-not [string]::IsNullOrEmpty($Script:ScriptDir)) {
            $cand = Join-Path $Script:ScriptDir "agy-zh-manage.cmd"
            if (Test-Path -LiteralPath $cand -PathType Leaf) {
                $localCmd = $cand
            }
        }
        if (-not [string]::IsNullOrEmpty($localCmd)) {
            $srcCmdNorm = [System.IO.Path]::GetFullPath($localCmd).ToLowerInvariant()
            $dstCmdNorm = [System.IO.Path]::GetFullPath($Script:TargetCmd).ToLowerInvariant()
            if ($srcCmdNorm -ne $dstCmdNorm) {
                Copy-Item -LiteralPath $localCmd -Destination $Script:TargetCmd -Force
            }
        } elseif ($syncedPs1) {
            Write-DefaultCmdWrapper $Script:TargetCmd
        }
    }
}

function Get-ShortcutPath {
    $outDir = if (-not [string]::IsNullOrWhiteSpace($env:AGY_ZH_SHORTCUT_DIR)) {
        $env:AGY_ZH_SHORTCUT_DIR
    } else {
        Join-Path $env:USERPROFILE "Desktop"
    }
    return Join-Path $outDir "Antigravity 中文.lnk"
}

function Invoke-AgyShortcut([bool]$RemoveShortcut) {
    $argsList = @("shortcut")
    if (-not [string]::IsNullOrWhiteSpace($Script:EffectiveApp)) {
        $argsList += @("--app", $Script:EffectiveApp)
    }
    if (-not [string]::IsNullOrWhiteSpace($env:AGY_ZH_SHORTCUT_DIR)) {
        $argsList += @("--output", $env:AGY_ZH_SHORTCUT_DIR)
    }
    if ($RemoveShortcut) {
        $argsList += "--remove"
    }
    $res = Invoke-NativeCommand $Script:TargetBin $argsList
    if ($res.ExitCode -ne 0) {
        throw "执行 agy-zh shortcut 失败: $($res.Output)"
    }
    return $res.Output
}

function Refresh-ExistingShortcut {
    $lnk = Get-ShortcutPath
    if (Test-Path -LiteralPath $lnk) {
        Write-Host "检测到已存在快捷方式 ($lnk)，正在刷新内嵌稳定副本..."
        try {
            $null = Invoke-AgyShortcut $false
            Write-Host "已刷新快捷方式 -> $lnk"
        } catch {
            Write-Warning "快捷方式刷新失败: $($_.Exception.Message)"
        }
    }
}

function Ensure-BinaryInstalled {
    if ((Test-Path -LiteralPath $Script:TargetBin) -and [string]::IsNullOrWhiteSpace($From) -and [string]::IsNullOrWhiteSpace($Version)) {
        return
    }
    if (-not [string]::IsNullOrWhiteSpace($From)) {
        Install-FromLocalFile $From
        Sync-ManageScripts
        Ensure-UserPathConfigured
        Refresh-ExistingShortcut
        return
    }

    $localCandidate = ""
    if ([string]::IsNullOrWhiteSpace($Version) -and (Test-IsInteractive)) {
        $localCandidate = Discover-LocalAsset
    }

    if (-not [string]::IsNullOrEmpty($localCandidate)) {
        Write-Host "检测到本地安装包: $localCandidate"
        Write-Host "  1. 迁移并重命名本地文件为 $($Script:TargetBin)（推荐）"
        Write-Host "  2. 从 GitHub Releases 在线下载最新版"
        $srcChoice = Read-Prompt "请选择来源 [1/2，默认 1]: "
        if ($srcChoice -eq "2") {
            Install-FromRelease
        } else {
            Install-FromLocalFile $localCandidate
        }
    } else {
        Install-FromRelease
    }
    Sync-ManageScripts
    Ensure-UserPathConfigured
    Refresh-ExistingShortcut
}

function Invoke-DoctorParsed {
    $argsList = @("doctor")
    if (-not [string]::IsNullOrWhiteSpace($Script:EffectiveApp)) {
        $argsList += @("--app", $Script:EffectiveApp)
    }
    $res = Invoke-NativeCommand $Script:TargetBin $argsList
    if ($res.ExitCode -ne 0) {
        return @{
            Ok = $false
            Error = $res.Output
            Data = $null
        }
    }
    try {
        $parsed = $res.Output | ConvertFrom-Json
        return @{
            Ok = $true
            Error = ""
            Data = $parsed
        }
    } catch {
        return @{
            Ok = $false
            Error = $res.Output
            Data = $null
        }
    }
}

function Do-InstallOrUpdate([bool]$ForceOnline) {
    if (-not [string]::IsNullOrWhiteSpace($From)) {
        Install-FromLocalFile $From
    } elseif ($ForceOnline -or -not [string]::IsNullOrWhiteSpace($Version)) {
        Install-FromRelease
    } else {
        $localCandidate = ""
        if (Test-IsInteractive) {
            $localCandidate = Discover-LocalAsset
        }
        if (-not [string]::IsNullOrEmpty($localCandidate)) {
            Write-Host "检测到本地安装包: $localCandidate"
            Write-Host "  1. 迁移并重命名本地文件为 $($Script:TargetBin)（推荐）"
            Write-Host "  2. 从 GitHub Releases 在线下载最新版"
            $srcChoice = Read-Prompt "请选择来源 [1/2，默认 1]: "
            if ($srcChoice -eq "2") {
                Install-FromRelease
            } else {
                Install-FromLocalFile $localCandidate
            }
        } else {
            Install-FromRelease
        }
    }
    Sync-ManageScripts
    Ensure-UserPathConfigured
    Refresh-ExistingShortcut
}

function Do-ModeCdp {
    Ensure-BinaryInstalled
    $doc = Invoke-DoctorParsed
    if ($doc.Ok) {
        $patched = [bool](Get-JsonProp $doc.Data "patched")
        if ($patched) {
            Write-Warning "检测到当前 Antigravity 已安装静态 ASAR 补丁，建议先还原官方原版再切换 CDP 动态模式。"
            if (Test-IsInteractive) {
                $ans = Read-Prompt "是否立即执行 agy-zh.exe restore 还原官方原版？[Y/n]: "
                if ($ans.ToLowerInvariant() -eq "n" -or $ans.ToLowerInvariant() -eq "no") {
                    throw "用户取消切换，请先运行 restore 后再试"
                }
                Do-Restore | Out-Null
            } else {
                throw "请先执行 agy-zh-manage.cmd -Restore 还原官方原版后再启用 CDP 模式"
            }
        }
    }

    Write-Host "正在生成「Antigravity 中文」桌面快捷方式..."
    $scOut = Invoke-AgyShortcut $false
    if (-not [string]::IsNullOrWhiteSpace($scOut)) {
        Write-Host $scOut
    }

    if (Test-IsInteractive) {
        $launch = Read-Prompt "快捷方式已就绪，是否立即以动态汉化模式启动 Antigravity？[y/N]: "
        if ($launch.ToLowerInvariant() -eq "y" -or $launch.ToLowerInvariant() -eq "yes") {
            $runArgs = @("run")
            if (-not [string]::IsNullOrWhiteSpace($Script:EffectiveApp)) {
                $runArgs += @("--app", $Script:EffectiveApp)
            }
            $runRes = Invoke-NativeCommand $Script:TargetBin $runArgs
            if ($runRes.ExitCode -ne 0) {
                throw "启动动态汉化失败: $($runRes.Output)"
            }
            Write-Host $runRes.Output
        }
    }
}

function Do-ModeAsar {
    Ensure-BinaryInstalled
    $doc = Invoke-DoctorParsed
    $staleVer = ""
    if ($doc.Ok) {
        $v = Get-JsonProp $doc.Data "stale_backup_version"
        if ($null -ne $v) {
            $staleVer = [string]$v
        }
    }

    $repStale = $ReplaceStaleBackup.IsPresent
    if (-not [string]::IsNullOrWhiteSpace($staleVer) -and -not $repStale) {
        Write-Warning "检测到旧版本残留备份 ($staleVer)。"
        if (Test-IsInteractive) {
            $repAns = Read-Prompt "是否自动替换旧版本备份并继续安装？[Y/n]: "
            if ($repAns.ToLowerInvariant() -ne "n" -and $repAns.ToLowerInvariant() -ne "no") {
                $repStale = $true
            }
        }
    }

    $argsList = @("install")
    if (-not [string]::IsNullOrWhiteSpace($Script:EffectiveApp)) {
        $argsList += @("--app", $Script:EffectiveApp)
    }
    if ($repStale) {
        $argsList += "--replace-stale-backup"
    }
    $res = Invoke-NativeCommand $Script:TargetBin $argsList
    if ($res.ExitCode -ne 0) {
        throw "静态 ASAR 安装失败: $($res.Output)"
    }
    Write-Host $res.Output
}

function Do-Status {
    $localVer = Get-LocalVersion
    if ([string]::IsNullOrEmpty($localVer)) {
        $localVer = "未安装"
    }

    $remoteVer = ""
    try {
        $remoteVer = Resolve-LatestTag
    } catch {
        $remoteVer = "获取失败（离线或网络受限）"
    }

    $lnk = Get-ShortcutPath
    $scDisp = if (Test-Path -LiteralPath $lnk) { $lnk } else { "未生成" }
    $mirrorDisp = if ([string]::IsNullOrWhiteSpace($Script:EffectiveMirror)) { "直连 GitHub (无镜像)" } else { $Script:EffectiveMirror }
    $appDisp = if ([string]::IsNullOrWhiteSpace($Script:EffectiveApp)) { "自动探测" } else { $Script:EffectiveApp }

    Write-Host "=== agy-zh 状态总览 ==="
    Write-Host "安装目录:       $($Script:EffectiveInstallDir)"
    Write-Host "可执行文件:     $($Script:TargetBin)"
    Write-Host "维护脚本:       $($Script:TargetScript)"
    Write-Host "本地版本:       $localVer"
    Write-Host "云端最新版本:   $remoteVer"
    Write-Host "下载加速镜像:   $mirrorDisp"
    Write-Host "目标应用路径:   $appDisp"
    Write-Host "快捷方式状态:   $scDisp"
    Write-Host "守护日志路径:   $($Script:LogFile)"

    if (Test-Path -LiteralPath $Script:TargetBin) {
        Write-Host ""
        Write-Host "=== Antigravity 诊断摘要 (agy-zh doctor) ==="
        $doc = Invoke-DoctorParsed
        if ($doc.Ok) {
            $d = $doc.Data
            Write-Host "应用目录:       $(Get-JsonProp $d 'app')"
            Write-Host "客户端版本:     $(Get-JsonProp $d 'version') ($(Get-JsonProp $d 'compatibility_mode'))"
            Write-Host "静态补丁状态:   $(Get-JsonProp $d 'patched') (备份: $(Get-JsonProp $d 'backup'))"
            Write-Host "CDP 守护运行中: $(Get-JsonProp $d 'cdp_daemon_running')"
        } else {
            Write-Warning "doctor 检查未通过: $($doc.Error)"
        }
    }
}

function Do-Logs {
    if (-not (Test-Path -LiteralPath $Script:LogFile -PathType Leaf)) {
        Write-Host "暂无守护日志: $($Script:LogFile)"
        return
    }
    Write-Host "=== 最近 50 行日志 ($($Script:LogFile)) ==="
    Get-Content -LiteralPath $Script:LogFile -Tail 50 -Encoding UTF8
}

function Do-ClearLogs {
    if (Test-Path -LiteralPath $Script:LogFile -PathType Leaf) {
        Clear-Content -LiteralPath $Script:LogFile
        Write-Host "已清空日志: $($Script:LogFile)"
    } else {
        Write-Host "日志文件不存在: $($Script:LogFile)"
    }
}

function Do-Restore {
    if (-not (Test-Path -LiteralPath $Script:TargetBin)) {
        throw "未找到 $($Script:TargetBin)，请先安装 agy-zh.exe"
    }
    $argsList = @("restore")
    if (-not [string]::IsNullOrWhiteSpace($Script:EffectiveApp)) {
        $argsList += @("--app", $Script:EffectiveApp)
    }
    $res = Invoke-NativeCommand $Script:TargetBin $argsList
    if ($res.ExitCode -ne 0) {
        throw "还原官方原版失败: $($res.Output)"
    }
    Write-Host $res.Output
}

function Do-Uninstall {
    if (Test-Path -LiteralPath $Script:TargetBin) {
        $null = Invoke-NativeCommand $Script:TargetBin @("stop")

        $lnk = Get-ShortcutPath
        if (Test-Path -LiteralPath $lnk) {
            Write-Host "正在移除快捷方式: $lnk"
            try {
                $null = Invoke-AgyShortcut $true
            } catch {
                Remove-Item -LiteralPath $lnk -Force -ErrorAction SilentlyContinue
            }
        }

        $shouldRestore = $RestoreApp.IsPresent
        if (-not $shouldRestore) {
            $doc = Invoke-DoctorParsed
            if ($doc.Ok -and [bool](Get-JsonProp $doc.Data "patched") -and (Test-IsInteractive)) {
                $rAns = Read-Prompt "检测到 Antigravity 仍保留静态补丁，卸载前是否先还原为官方原版？[Y/n]: "
                if ($rAns.ToLowerInvariant() -ne "n" -and $rAns.ToLowerInvariant() -ne "no") {
                    $shouldRestore = $true
                }
            }
        }
        if ($shouldRestore) {
            Write-Host "正在还原 Antigravity 官方原版..."
            try {
                Do-Restore
            } catch {
                Write-Warning "还原应用失败，将继续清理本地工具文件: $($_.Exception.Message)"
            }
        }
    }

    Remove-UserPathEntry

    foreach ($f in @($Script:TargetBin, $Script:TargetScript, $Script:TargetCmd, $Script:ConfFile)) {
        if (Test-Path -LiteralPath $f) {
            Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue
        }
    }
    Write-Host "已完成彻底卸载（自定义词表与日志如存在未主动删除）。"
}

function Interactive-ConfigureSettings {
    Write-Host ""
    Write-Host "=== 目录与下载源设置 ==="
    Write-Host "当前安装目录: $($Script:EffectiveInstallDir)"
    $mDisp = if ([string]::IsNullOrWhiteSpace($Script:EffectiveMirror)) { "直连 GitHub" } else { $Script:EffectiveMirror }
    $aDisp = if ([string]::IsNullOrWhiteSpace($Script:EffectiveApp)) { "自动探测" } else { $Script:EffectiveApp }
    Write-Host "当前加速镜像: $mDisp"
    Write-Host "当前应用路径: $aDisp"
    Write-Host ""

    $newDir = Read-Prompt "输入新的安装目录（留空保持不变）: "
    if (-not [string]::IsNullOrWhiteSpace($newDir)) {
        $resolvedNewDir = Expand-UserPath $newDir
        if ($resolvedNewDir -ne $Script:EffectiveInstallDir) {
            if (-not (Test-Path -LiteralPath $resolvedNewDir)) {
                New-Item -ItemType Directory -Path $resolvedNewDir -Force | Out-Null
            }
            if (Test-Path -LiteralPath $Script:TargetBin) {
                Copy-Item -LiteralPath $Script:TargetBin -Destination (Join-Path $resolvedNewDir "agy-zh.exe") -Force
                Remove-Item -LiteralPath $Script:TargetBin -Force -ErrorAction SilentlyContinue
            }
            if (Test-Path -LiteralPath $Script:TargetScript) {
                Copy-Item -LiteralPath $Script:TargetScript -Destination (Join-Path $resolvedNewDir "agy-zh-manage.ps1") -Force
                Remove-Item -LiteralPath $Script:TargetScript -Force -ErrorAction SilentlyContinue
            }
            if (Test-Path -LiteralPath $Script:TargetCmd) {
                Copy-Item -LiteralPath $Script:TargetCmd -Destination (Join-Path $resolvedNewDir "agy-zh-manage.cmd") -Force
                Remove-Item -LiteralPath $Script:TargetCmd -Force -ErrorAction SilentlyContinue
            }
            $Script:EffectiveInstallDir = $resolvedNewDir
            $Script:TargetBin = Join-Path $Script:EffectiveInstallDir "agy-zh.exe"
            $Script:TargetScript = Join-Path $Script:EffectiveInstallDir "agy-zh-manage.ps1"
            $Script:TargetCmd = Join-Path $Script:EffectiveInstallDir "agy-zh-manage.cmd"
            Ensure-UserPathConfigured
        }
    }

    $newMirror = Read-Prompt "输入 GitHub 加速镜像前缀（输入 none 恢复直连，留空保持不变）: "
    if (-not [string]::IsNullOrWhiteSpace($newMirror)) {
        if ($newMirror.Trim().ToLowerInvariant() -eq "none") {
            $Script:EffectiveMirror = ""
        } else {
            $Script:EffectiveMirror = $newMirror.Trim().TrimEnd("/")
        }
    }

    $newApp = Read-Prompt "输入 Antigravity 应用路径（输入 auto 恢复自动探测，留空保持不变）: "
    if (-not [string]::IsNullOrWhiteSpace($newApp)) {
        if ($newApp.Trim().ToLowerInvariant() -eq "auto") {
            $Script:EffectiveApp = ""
        } else {
            $Script:EffectiveApp = Expand-UserPath $newApp
        }
    }

    Save-Config
    Write-Host "设置已保存至 $($Script:ConfFile)"
}

function Show-InteractiveMenu {
    while ($true) {
        Resolve-EffectiveSettings
        $localVer = Get-LocalVersion
        $verDisp = if ([string]::IsNullOrEmpty($localVer)) { "未安装" } else { $localVer }
        $mDisp = if ([string]::IsNullOrWhiteSpace($Script:EffectiveMirror)) { "直连 GitHub" } else { $Script:EffectiveMirror }

        Write-Host ""
        Write-Host "================================================"
        Write-Host "  Antigravity 简体中文汉化 一键维护工具 (v$($Script:ScriptVersion))"
        Write-Host "================================================"
        Write-Host "  安装位置: $($Script:EffectiveInstallDir)"
        Write-Host "  当前版本: $verDisp | 镜像源: $mDisp"
        Write-Host "------------------------------------------------"
        Write-Host "  1. 一键部署 CDP 动态汉化（推荐：不改原包 + 桌面快捷方式）"
        Write-Host "  2. 一键安装静态 ASAR 安全补丁（含原生右键/托盘菜单）"
        Write-Host "  3. 安装 / 迁移本地文件 / 在线更新 agy-zh.exe"
        Write-Host "  4. 查看状态总览与诊断报告"
        Write-Host "  5. 查看或清空 CDP 守护日志"
        Write-Host "  6. 还原 Antigravity 为官方原版"
        Write-Host "  7. 设置安装目录 / 下载镜像源 / 应用路径"
        Write-Host "  8. 彻底卸载 agy-zh 与本维护脚本"
        Write-Host "  0. 退出"
        Write-Host ""

        $choice = Read-Prompt "请输入编号 [0-8]: "
        try {
            switch ($choice) {
                "1" { Do-ModeCdp }
                "2" { Do-ModeAsar }
                "3" { Do-InstallOrUpdate $false }
                "4" { Do-Status }
                "5" {
                    Do-Logs
                    $clr = Read-Prompt "是否清空日志文件？[y/N]: "
                    if ($clr.ToLowerInvariant() -eq "y" -or $clr.ToLowerInvariant() -eq "yes") {
                        Do-ClearLogs
                    }
                }
                "6" { Do-Restore }
                "7" { Interactive-ConfigureSettings }
                "8" {
                    $confirm = Read-Prompt "确认彻底卸载 agy-zh 与维护脚本？[y/N]: "
                    if ($confirm.ToLowerInvariant() -eq "y" -or $confirm.ToLowerInvariant() -eq "yes") {
                        Do-Uninstall
                        return
                    }
                }
                "0" { return }
                "" { return }
                default { Write-Warning "无效编号: $choice" }
            }
        } catch {
            Write-Host "错误: $($_.Exception.Message)" -ForegroundColor Red
        }
    }
}

$exitCode = 0
try {
    if ($Help) {
        Show-Usage
        return
    }

    Resolve-EffectiveSettings

    if (-not $Uninstall) {
        if (-not [string]::IsNullOrWhiteSpace($InstallDir) -or $PSBoundParameters.ContainsKey("Mirror") -or -not [string]::IsNullOrWhiteSpace($App)) {
            Save-Config
        }
    }

    if ($Uninstall) {
        Do-Uninstall
    } elseif ($Update) {
        Do-InstallOrUpdate $true
    } elseif ($Mode -eq "cdp") {
        Do-ModeCdp
    } elseif ($Mode -eq "asar") {
        Do-ModeAsar
    } elseif ($Status) {
        Do-Status
    } elseif ($Logs) {
        Do-Logs
    } elseif ($ClearLogs) {
        Do-ClearLogs
    } elseif ($Restore) {
        Do-Restore
    } elseif (-not [string]::IsNullOrWhiteSpace($From) -or -not [string]::IsNullOrWhiteSpace($Version)) {
        Do-InstallOrUpdate $false
    } else {
        if (Test-IsInteractive) {
            Show-InteractiveMenu
        } else {
            Do-InstallOrUpdate $false
            Write-Host "基础安装已完成，后续可运行 agy-zh-manage.cmd 打开维护菜单。"
        }
    }
} catch {
    Write-Host "错误: $($_.Exception.Message)" -ForegroundColor Red
    $exitCode = 1
} finally {
    Cleanup-TempDir
}

if (-not $Script:IsPipedIex -and $exitCode -ne 0) {
    exit $exitCode
}
