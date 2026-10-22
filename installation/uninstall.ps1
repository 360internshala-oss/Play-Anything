# Remove Play Anything commands, managed runtimes, PATH, and per-user environment.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$InstallHome = if ($env:PLAY_ANYTHING_HOME) {
    $env:PLAY_ANYTHING_HOME
} else {
    Join-Path $env:LOCALAPPDATA 'Programs\play-anything'
}
$CommandDir = if ($env:PLAY_ANYTHING_COMMAND_DIR) {
    $env:PLAY_ANYTHING_COMMAND_DIR
} else {
    Join-Path $InstallHome 'commands'
}

function Write-Step([string]$Message) {
    Write-Host "[play-anything] $Message" -ForegroundColor Cyan
}

$FullInstallHome = [IO.Path]::GetFullPath($InstallHome)
if ($FullInstallHome.TrimEnd('\') -eq [IO.Path]::GetPathRoot($FullInstallHome).TrimEnd('\')) {
    throw "Refusing to remove unsafe installation path: $FullInstallHome"
}

foreach ($CommandName in 'play-anything.exe', 'Play-Anything.exe') {
    $Command = Join-Path $CommandDir $CommandName
    if (Test-Path -LiteralPath $Command -PathType Leaf) {
        Remove-Item -LiteralPath $Command -Force
        Write-Step "Removed $Command"
    }
}

if (Test-Path -LiteralPath $FullInstallHome) {
    Remove-Item -LiteralPath $FullInstallHome -Recurse -Force
    Write-Step "Removed $FullInstallHome"
}

$UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$NormalizedCommandDir = $CommandDir.TrimEnd('\')
if ($null -ne $UserPath) {
    $PathEntries = @($UserPath -split ';' | Where-Object {
        -not [string]::IsNullOrWhiteSpace($_) -and $_.TrimEnd('\') -ne $NormalizedCommandDir
    })
    $NewUserPath = $PathEntries -join ';'
    if ($NewUserPath -ne $UserPath) {
        [Environment]::SetEnvironmentVariable('Path', $NewUserPath, 'User')
    }
}
$env:PATH = (@($env:PATH -split ';' | Where-Object {
    -not [string]::IsNullOrWhiteSpace($_) -and $_.TrimEnd('\') -ne $NormalizedCommandDir
}) -join ';')

foreach ($Variable in 'PLAY_ANYTHING_ROOT', 'PLAY_ANYTHING_TOOLS', 'PLAY_ANYTHING_HOME') {
    [Environment]::SetEnvironmentVariable($Variable, $null, 'User')
    Remove-Item "Env:$Variable" -ErrorAction SilentlyContinue
}

Write-Step 'Play Anything is uninstalled. Restart your terminal to refresh PATH.'
Write-Step 'All application files and managed Windows media runtimes were removed.'
