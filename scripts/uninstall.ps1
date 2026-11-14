# Tether Uninstaller for Windows
#
#   irm https://raw.githubusercontent.com/AMG555/Tether/main/scripts/uninstall.ps1 | iex
#   Or locally: .\scripts\uninstall.ps1 [-PurgeData]
#
# Removes autostart services, desktop shortcuts, Start menu items, and binaries.
# Passing -PurgeData also wipes local encrypted vault records and encryption keys.

param(
    [switch]$PurgeData
)

$ErrorActionPreference = 'SilentlyContinue'
$BinDir = if ($env:TETHER_BIN_DIR) { $env:TETHER_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Tether\bin' }

Write-Host "tether uninstaller" -ForegroundColor Cyan
Write-Host "  stopping running processes..."

Get-Process tether, tether-desktop -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

# 1. Remove autostart
Write-Host "  removing autostart shortcuts..."
$startupDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup'
Remove-Item (Join-Path $startupDir 'tether.url') -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $startupDir 'tether.lnk') -Force -ErrorAction SilentlyContinue

# 2. Remove Desktop & Start Menu shortcuts
Write-Host "  removing desktop & start menu shortcuts..."
$desktopDir = [Environment]::GetFolderPath('Desktop')
Remove-Item (Join-Path $desktopDir 'Tether Desktop.lnk') -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $desktopDir 'Tether.lnk') -Force -ErrorAction SilentlyContinue

$programsDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
Remove-Item (Join-Path $programsDir 'Tether Desktop.lnk') -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $programsDir 'Tether.lnk') -Force -ErrorAction SilentlyContinue

# 3. Remove installed binaries
if (Test-Path $BinDir) {
    Write-Host "  removing binaries from $BinDir..."
    Remove-Item (Join-Path $BinDir 'tether.exe') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $BinDir 'tether-desktop.exe') -Force -ErrorAction SilentlyContinue
    # If folder empty, remove it
    if ((Get-ChildItem -Path $BinDir).Count -eq 0) {
        Remove-Item $BinDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# 4. Optional: Purge Vault & Key Data
if ($PurgeData) {
    Write-Host "  purging local encrypted vault data and recovery keys..." -ForegroundColor Yellow
    $vaultDirs = @(
        (Join-Path $env:LOCALAPPDATA 'tether_data'),
        (Join-Path $env:APPDATA 'tether'),
        (Join-Path $env:LOCALAPPDATA 'tether')
    )
    foreach ($v in $vaultDirs) {
        if (Test-Path $v) {
            Remove-Item $v -Recurse -Force -ErrorAction SilentlyContinue
            Write-Host "    purged: $v"
        }
    }
} else {
    Write-Host "  vault data preserved. (Pass -PurgeData to erase stored texts & keys)" -ForegroundColor Gray
}

Write-Host ""
Write-Host "Tether uninstallation complete." -ForegroundColor Green
