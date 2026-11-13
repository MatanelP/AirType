# Local resolver tests only: dot-sourcing defines functions without installing.
# Run with: pwsh -NoProfile -File scripts/installers-windows.test.ps1
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/install.ps1"

$script:Calls = @()
function Invoke-RestMethod {
    param([switch]$UseBasicParsing, [string]$Uri, $Headers, [int]$TimeoutSec)
    $script:Calls += $Uri
    if ($Uri.EndsWith('/releases/latest')) {
        if ($script:LatestError) { throw 'injected latest API failure' }
        return $script:Latest
    }
    if ($Uri.EndsWith('/releases?per_page=100&page=1')) {
        if ($script:ListError) { throw 'injected release list failure' }
        return ,$script:Releases
    }
    throw "Unexpected request: $Uri"
}

function Assert-Equal($Actual, $Expected) {
    if ($Actual -cne $Expected) { throw "Expected '$Expected', got '$Actual'" }
}

function New-TestRelease($Tag, $Installer, $Draft = $false, $Prerelease = $false) {
    [pscustomobject]@{ tag_name = $Tag; draft = $Draft; prerelease = $Prerelease
        assets = @([pscustomobject]@{ name = Get-AirTypeAssetName $Tag $Installer; state = 'uploaded' }) }
}

function Reset-TestApi {
    $script:Calls = @()
    $script:LatestError = $false
    $script:ListError = $false
}

foreach ($installer in @('msi', 'nsis')) {
    Reset-TestApi
    $script:Latest = New-TestRelease 'v1.10.0' $installer
    Assert-Equal (Resolve-AirTypeVersion 'owner/repo' '' $installer) 'v1.10.0'
    Assert-Equal $script:Calls.Count 1

    Reset-TestApi
    $script:Latest = New-TestRelease 'v1.11.0' $installer
    $script:Latest.assets = @()
    $script:Releases = @(
        (New-TestRelease 'v1.9.0' $installer),
        (New-TestRelease 'v1.10.0' $installer),
        (New-TestRelease 'v1.12.0' $installer),
        (New-TestRelease 'v1.11.0' $installer $true),
        (New-TestRelease 'v1.11.0' $installer $false $true)
    )
    Assert-Equal (Resolve-AirTypeVersion 'owner/repo' '' $installer) 'v1.10.0'
    Assert-Equal $script:Calls.Count 2

    Reset-TestApi
    Assert-Equal (Resolve-AirTypeVersion 'owner/repo' 'v1.11.0' $installer) 'v1.11.0'
    Assert-Equal $script:Calls.Count 0

    Reset-TestApi
    $script:LatestError = $true
    $failed = $false
    try { Resolve-AirTypeVersion 'owner/repo' '' $installer | Out-Null } catch { $failed = $true }
    Assert-Equal $failed $true
    Assert-Equal $script:Calls.Count 1

    Reset-TestApi
    $script:Latest = [pscustomobject]@{ tag_name = 'v1.10.0'; assets = @() }
    $failed = $false
    try { Resolve-AirTypeVersion 'owner/repo' '' $installer | Out-Null } catch { $failed = $true }
    Assert-Equal $failed $true
    Assert-Equal $script:Calls.Count 1

    Reset-TestApi
    $script:Latest = New-TestRelease 'v1.10.0' $installer
    $script:Latest.assets = @()
    $script:ListError = $true
    $failed = $false
    try { Resolve-AirTypeVersion 'owner/repo' '' $installer | Out-Null } catch { $failed = $true }
    Assert-Equal $failed $true
    Assert-Equal $script:Calls.Count 2

    Reset-TestApi
    $script:Releases = @((New-TestRelease 'v1.9.0' $installer))
    $script:Releases[0].assets[0].state = 'new'
    $failed = $false
    try { Resolve-AirTypeVersion 'owner/repo' '' $installer | Out-Null } catch { $failed = $true }
    Assert-Equal $failed $true
}
Write-Host 'Windows installer resolver: MSI/NSIS latest, bounded fallback, exact pins, metadata/API/upload failures passed'
