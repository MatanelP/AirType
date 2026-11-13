# AirType installer — Windows
#
# Usage (PowerShell):
#   irm https://raw.githubusercontent.com/MatanelP/AirType/main/scripts/install.ps1 | iex
#
# Env overrides:
#   $env:AIRTYPE_VERSION   Exact tag (default: newest compatible stable release)
#   $env:AIRTYPE_REPO      owner/name (default: MatanelP/AirType)
#   $env:AIRTYPE_INSTALLER Installer type: 'msi' (default) or 'nsis'

#Requires -Version 5.1

$ErrorActionPreference = 'Stop'

function Info($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Warn($msg) { Write-Host "!!  $msg" -ForegroundColor Yellow }
function Die($msg)  { Write-Host "xx  $msg" -ForegroundColor Red; exit 1 }

function Get-AirTypeAssetName([string]$Tag, [string]$Installer) {
    $number = if ($Tag.StartsWith('v')) { $Tag.Substring(1) } else { $Tag }
    if ($Installer -eq 'msi') { return "AirType_${number}_x64_en-US.msi" }
    return "AirType_${number}_x64-setup.exe"
}

function Test-AirTypeStableTag([string]$Tag) {
    return $Tag -cmatch '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
}

function Compare-AirTypeVersions([string]$Left, [string]$Right) {
    $leftParts = $Left.Substring(1).Split('.')
    $rightParts = $Right.Substring(1).Split('.')
    for ($i = 0; $i -lt 3; $i++) {
        if ($leftParts[$i].Length -ne $rightParts[$i].Length) {
            return $leftParts[$i].Length.CompareTo($rightParts[$i].Length)
        }
        $order = [string]::CompareOrdinal($leftParts[$i], $rightParts[$i])
        if ($order -ne 0) { return $order }
    }
    return 0
}

function Assert-AirTypeRelease($Release) {
    if ($null -eq $Release -or $Release.tag_name -isnot [string] -or
        $Release.draft -isnot [bool] -or $Release.prerelease -isnot [bool] -or
        $Release.assets -isnot [array]) {
        throw 'Invalid GitHub release metadata'
    }
}

function Test-AirTypeInstaller($Release, [string]$Installer) {
    $name = Get-AirTypeAssetName $Release.tag_name $Installer
    return @($Release.assets | Where-Object { $_.name -ceq $name -and $_.state -ceq 'uploaded' }).Count -gt 0
}

function Resolve-AirTypeVersion([string]$Repo, [string]$Version, [string]$Installer) {
    # Pins never query another release, even if their download later fails.
    if ($Version) { return $Version }
    $headers = @{ 'User-Agent' = 'airtype-installer' }
    $latest = Invoke-RestMethod -UseBasicParsing -TimeoutSec 60 `
        -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers $headers
    Assert-AirTypeRelease $latest
    if (-not (Test-AirTypeStableTag $latest.tag_name) -or $latest.draft -or $latest.prerelease) {
        throw 'GitHub latest release must have a stable version tag'
    }
    if (Test-AirTypeInstaller $latest $Installer) { return $latest.tag_name }

    Info "$($latest.tag_name) does not yet contain this installer; checking up to 100 releases..."
    $releases = @(Invoke-RestMethod -UseBasicParsing -TimeoutSec 60 `
        -Uri "https://api.github.com/repos/$Repo/releases?per_page=100&page=1" -Headers $headers)
    # Invoke-RestMethod can return the JSON array as one pipeline object.
    if ($releases.Count -eq 1 -and $releases[0] -is [array]) { $releases = $releases[0] }
    if ($releases.Count -gt 100) { throw 'GitHub release scan exceeded 100 releases' }
    $best = $null
    foreach ($release in $releases) {
        Assert-AirTypeRelease $release
        if ($release.draft -or $release.prerelease -or -not (Test-AirTypeStableTag $release.tag_name)) { continue }
        if ((Compare-AirTypeVersions $release.tag_name $latest.tag_name) -gt 0) { continue }
        if (-not (Test-AirTypeInstaller $release $Installer)) { continue }
        if (-not $best -or (Compare-AirTypeVersions $release.tag_name $best) -gt 0) { $best = $release.tag_name }
    }
    if (-not $best) { throw 'No compatible stable installer found in the first 100 releases' }
    return $best
}

# Dot-sourcing supports the local mocked resolver tests without installing.
if ($MyInvocation.InvocationName -eq '.') { return }

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

try {
    $Version = Resolve-AirTypeVersion $Repo $Version $Installer
} catch {
    Die "could not resolve a compatible release: $_"
}
if (-not $Version) { Die 'could not determine latest release tag' }
Info "Installing AirType $Version on Windows x64 ($Installer)"

$asset = Get-AirTypeAssetName $Version $Installer

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
