# Play Anything one-line installer for Windows 10/11. No Rust toolchain required.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Repository = '360internshala-oss/Play-Anything'
$ReleaseBase = if ($env:PLAY_ANYTHING_RELEASE_BASE) {
    $env:PLAY_ANYTHING_RELEASE_BASE
} else {
    "https://github.com/$Repository/releases/latest/download"
}
$InstallHome = if ($env:PLAY_ANYTHING_HOME) {
    $env:PLAY_ANYTHING_HOME
} else {
    Join-Path $env:LOCALAPPDATA 'Programs\play-anything'
}
$Tools = Join-Path $InstallHome '.tools'
$CommandDir = if ($env:PLAY_ANYTHING_COMMAND_DIR) {
    $env:PLAY_ANYTHING_COMMAND_DIR
} else {
    Join-Path $InstallHome 'commands'
}
$TempDirectory = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())

function Write-Step([string]$Message) {
    Write-Host "[play-anything] $Message" -ForegroundColor Cyan
}

function Get-Download([string]$Url, [string]$Destination) {
    Write-Step "Downloading $([IO.Path]::GetFileName($Destination))"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile "$Destination.part"
    Move-Item -Force "$Destination.part" $Destination
}

function Expand-SevenZip([string]$Archive, [string]$Destination) {
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    & tar.exe -xf $Archive -C $Destination
    if ($LASTEXITCODE -ne 0) {
        throw "Could not extract $Archive. Windows 10/11's tar.exe is required."
    }
}

function Install-PackedRuntime([string]$Archive, [string]$Runtime, [string]$ExecutableName) {
    $ExtractDirectory = Join-Path $TempDirectory ([IO.Path]::GetRandomFileName())
    Expand-SevenZip $Archive $ExtractDirectory
    $Executable = Get-ChildItem -Path $ExtractDirectory -Filter $ExecutableName -Recurse -File |
        Select-Object -First 1
    if ($null -eq $Executable) {
        throw "$ExecutableName was not found in $Archive"
    }
    New-Item -ItemType Directory -Force -Path $Runtime | Out-Null
    Copy-Item -Path (Join-Path $Executable.DirectoryName '*') -Destination $Runtime -Recurse -Force
    Remove-Item -Recurse -Force $ExtractDirectory
}

try {
    $OsArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($OsArchitecture) {
        'X64' { $Architecture = 'x86_64' }
        'Arm64' { $Architecture = 'aarch64' }
        default { throw "Unsupported Windows CPU architecture: $OsArchitecture" }
    }

    $ApplicationDirectory = Join-Path $InstallHome 'bin'
    $Application = Join-Path $ApplicationDirectory 'play-anything.exe'
    $ToolsBin = Join-Path $Tools 'bin'
    $MpvRuntime = Join-Path $Tools 'mpv-runtime'
    $FfmpegRuntime = Join-Path $Tools 'ffmpeg-runtime'
    New-Item -ItemType Directory -Force -Path $ApplicationDirectory, $ToolsBin, $CommandDir, $TempDirectory |
        Out-Null

    Write-Step "Installing Play Anything for windows/$Architecture"
    Get-Download "$ReleaseBase/play-anything-windows-$Architecture.exe" $Application
    & $Application --version | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'The downloaded application could not start' }

    if ($env:PLAY_ANYTHING_SKIP_MEDIA -ne '1') {
        $MpvRelease = '20261021'
        $MpvHash = 'e471496c56'
        $FfmpegHash = '9a861e968'
        $MpvExecutable = Join-Path $MpvRuntime 'mpv.exe'
        $FfmpegExecutable = Join-Path $FfmpegRuntime 'ffmpeg.exe'
        $YtDlpExecutable = Join-Path $ToolsBin 'yt-dlp.exe'

        if (-not (Test-Path $MpvExecutable)) {
            $MpvArchive = Join-Path $TempDirectory 'mpv.7z'
            Get-Download `
                "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/$MpvRelease/mpv-$Architecture-$MpvRelease-git-$MpvHash.7z" `
                $MpvArchive
            Install-PackedRuntime $MpvArchive $MpvRuntime 'mpv.exe'
        }
        if (-not (Test-Path $FfmpegExecutable)) {
            $FfmpegArchive = Join-Path $TempDirectory 'ffmpeg.7z'
            Get-Download `
                "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/$MpvRelease/ffmpeg-$Architecture-git-$FfmpegHash.7z" `
                $FfmpegArchive
            Install-PackedRuntime $FfmpegArchive $FfmpegRuntime 'ffmpeg.exe'
        }
        if (-not (Test-Path $YtDlpExecutable)) {
            Get-Download `
                'https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.21/yt-dlp.exe' `
                $YtDlpExecutable
        }
    }

    # Put the native executable itself on PATH. Windows command lookup is case-insensitive,
    # so play-anything and Play-Anything both open the same full-screen TUI directly.
    Copy-Item -Force $Application (Join-Path $CommandDir 'play-anything.exe')
    [Environment]::SetEnvironmentVariable('PLAY_ANYTHING_ROOT', $InstallHome, 'User')
    [Environment]::SetEnvironmentVariable('PLAY_ANYTHING_TOOLS', $Tools, 'User')
    $env:PLAY_ANYTHING_ROOT = $InstallHome
    $env:PLAY_ANYTHING_TOOLS = $Tools

    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($UserPath -split ';') -notcontains $CommandDir) {
        $NewUserPath = if ([string]::IsNullOrWhiteSpace($UserPath)) {
            $CommandDir
        } else {
            "$UserPath;$CommandDir"
        }
        [Environment]::SetEnvironmentVariable('Path', $NewUserPath, 'User')
    }
    if (($env:PATH -split ';') -notcontains $CommandDir) {
        $env:PATH = "$CommandDir;$env:PATH"
    }

    Write-Host ''
    Write-Step 'Installed successfully. Run: play-anything'
    Write-Host 'Both play-anything and Play-Anything open the full interactive TUI.'
} finally {
    Remove-Item -Recurse -Force $TempDirectory -ErrorAction SilentlyContinue
}
