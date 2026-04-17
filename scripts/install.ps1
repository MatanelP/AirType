# AirType installer — Windows
#
# Usage (PowerShell):
#   irm https://raw.githubusercontent.com/MatanelP/AirType/master/scripts/install.ps1 | iex
#
# Env overrides:
#   $env:AIRTYPE_VERSION   Tag to install (default: latest release)
#   $env:AIRTYPE_REPO      owner/name (default: MatanelP/AirType)
#   $env:AIRTYPE_INSTALLER Installer type: 'msi' (default) or 'nsis'

#Requires -Version 5.1

$ErrorActionPreference = 'Stop'

function Info($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Warn($msg) { Write-Host "!!  $msg" -ForegroundColor Yellow }
function Die($msg)  { Write-Host "xx  $msg" -ForegroundColor Red; exit 1 }

$Repo      = if ($env:AIRTYPE_REPO)      { $env:AIRTYPE_REPO }      else { 'MatanelP/AirType' }
$Version   = $env:AIRTYPE_VERSION
$Installer = if ($env:AIRTYPE_INSTALLER) { $env:AIRTYPE_INSTALLER } else { 'msi' }

if ($Installer -ne 'msi' -and $Installer -ne 'nsis') {
    Die "AIRTYPE_INSTALLER must be 'msi' or 'nsis' (got '$Installer')"
}

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne 'AMD64') {
    Die "unsupported arch: $arch (only x64 is published)"
}

if (-not $Version) {
    Info 'Resolving latest release…'
    try {
        $rel = Invoke-RestMethod -UseBasicParsing `
            -Uri "https://api.github.com/repos/$Repo/releases/latest" `
            -Headers @{ 'User-Agent' = 'airtype-installer' }
        $Version = $rel.tag_name
    } catch {
        Die "could not query GitHub API: $_"
    }
}
if (-not $Version) { Die 'could not determine latest release tag' }
$versionNum = $Version.TrimStart('v')

Info "Installing AirType $Version on Windows x64 ($Installer)"

if ($Installer -eq 'msi') {
    $asset = "AirType_${versionNum}_x64_en-US.msi"
} else {
    $asset = "AirType_${versionNum}_x64-setup.exe"
}

$url = "https://github.com/$Repo/releases/download/$Version/$asset"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("airtype-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
$out = Join-Path $tmp $asset

try {
    Info "Downloading $asset"
    Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $out

    Info 'Launching installer (may prompt for elevation)…'
    if ($Installer -eq 'msi') {
        # REINSTALL=ALL + REINSTALLMODE=amus forces Windows Installer to
        # overwrite the install even when the same ProductVersion is
        # already present, so re-running this script always results in a
        # clean install of the downloaded build.
        $proc = Start-Process -FilePath 'msiexec.exe' `
            -ArgumentList @('/i', "`"$out`"", '/qb', '/norestart',
                            'REINSTALL=ALL', 'REINSTALLMODE=amus') `
            -Wait -PassThru -Verb RunAs
    } else {
        $proc = Start-Process -FilePath $out `
            -ArgumentList @('/S') `
            -Wait -PassThru -Verb RunAs
    }
    if ($proc.ExitCode -ne 0) {
        Die "installer exited with code $($proc.ExitCode)"
    }
    Info 'Installed.'

    $launchCmd = 'Start-Process -FilePath "AirType"'
    $doLaunch = $false
    if ($env:AIRTYPE_NO_LAUNCH) {
        Info "Launch later from the Start menu or: $launchCmd"
    } elseif ([System.Environment]::UserInteractive -and $Host.UI.RawUI) {
        $ans = Read-Host '==> Launch AirType now? [Y/n]'
        if ($ans -eq '' -or $ans -match '^(y|yes)$') { $doLaunch = $true }
    } else {
        Info "Launch later from the Start menu or: $launchCmd"
    }

    if ($doLaunch) {
        Info 'Launching…'
        try {
            Start-Process -FilePath 'AirType' -ErrorAction Stop
        } catch {
            Warn "could not launch via 'AirType' on PATH. Opening from Start menu or re-login may be required."
        }
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
