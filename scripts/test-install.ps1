$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$installer = Join-Path $PSScriptRoot 'install.ps1'
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("tether-install-test-" + [guid]::NewGuid())
$archiveDir = Join-Path $work 'archive'
$binDir = Join-Path $work 'bin'
$archive = Join-Path $work 'tether-windows-x86_64.zip'
$installed = Join-Path $binDir 'tether.exe'
$originalUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$originalBaseUrl = $env:TETHER_BASE_URL
$originalBinDir = $env:TETHER_BIN_DIR
$originalRepo = $env:TETHER_REPO

New-Item -ItemType Directory -Path (Join-Path $archiveDir 'tether-windows-x86_64') -Force | Out-Null
New-Item -ItemType Directory -Path $binDir -Force | Out-Null
Set-Content -LiteralPath (Join-Path $archiveDir 'tether-windows-x86_64\tether.exe') -Value 'test binary'
Compress-Archive -Path (Join-Path $archiveDir 'tether-windows-x86_64') -DestinationPath $archive

$env:TETHER_BIN_DIR = $binDir
$env:TETHER_REPO = 'test/Tether'
$env:TETHER_BASE_URL = 'https://example.invalid/releases'

$testPath = if ($originalUserPath) { "$originalUserPath;$binDir" } else { $binDir }
[Environment]::SetEnvironmentVariable('Path', $testPath, 'User')

$global:ChecksumsMode = 'missing-file'
function Invoke-WebRequest {
    param(
        [string]$Uri,
        [string]$OutFile,
        [switch]$UseBasicParsing
    )

    if ($Uri.EndsWith('/checksums.txt')) {
        if ($global:ChecksumsMode -eq 'missing-file') {
            throw [System.Net.WebException]::new('checksums file unavailable')
        }
        if ($global:ChecksumsMode -eq 'missing-entry') {
            return [pscustomobject]@{ Content = "0000000000000000000000000000000000000000000000000000000000000000  another-asset.zip`n" }
        }
        if ($global:ChecksumsMode -eq 'mismatch') {
            return [pscustomobject]@{ Content = "0000000000000000000000000000000000000000000000000000000000000000  tether-windows-x86_64.zip`n" }
        }
        if ($global:ChecksumsMode -eq 'valid') {
            $hash = (Get-FileHash -Algorithm SHA256 -Path $archive).Hash.ToLowerInvariant()
            return [pscustomobject]@{ Content = "$hash  tether-windows-x86_64.zip`n" }
        }
        throw "Unexpected checksum test mode: $global:ChecksumsMode"
    }

    Copy-Item -LiteralPath $archive -Destination $OutFile -Force
}

$failures = 0
try {
    $cases = @(
        @{ Mode = 'missing-file'; Error = 'checksums file unavailable' },
        @{ Mode = 'missing-entry'; Error = 'checksums\.txt has no valid SHA-256 entry' },
        @{ Mode = 'mismatch'; Error = 'checksum mismatch' },
        @{ Mode = 'valid'; Error = '' }
    )
    foreach ($case in $cases) {
        $mode = $case.Mode
        $global:ChecksumsMode = $mode
        Remove-Item -LiteralPath $installed -Force -ErrorAction SilentlyContinue
        $output = $null
        $errorOutput = $null
        try {
            $output = & $installer *>&1 | Out-String
        } catch {
            $errorOutput = $_.ToString()
        }

        $installedFile = Test-Path -LiteralPath $installed
        if ($mode -eq 'valid') {
            if ($installedFile -and $output -match 'checksum  ok') {
                Write-Host 'PASS  installer accepts and verifies a valid checksum'
            } else {
                Write-Host "FAIL  installer rejected a valid checksum: $errorOutput"
                $failures++
            }
        } elseif ((-not $installedFile) -and $errorOutput -match $case.Error) {
            Write-Host "PASS  installer refuses $mode"
        } else {
            Write-Host "FAIL  installer did not refuse $mode"
            $failures++
        }
    }
} finally {
    [Environment]::SetEnvironmentVariable('Path', $originalUserPath, 'User')
    $env:TETHER_BASE_URL = $originalBaseUrl
    $env:TETHER_BIN_DIR = $originalBinDir
    $env:TETHER_REPO = $originalRepo
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

if ($failures -gt 0) {
    throw "$failures Windows installer checksum test(s) failed."
}

Write-Host 'Windows installer checksum tests passed.'
