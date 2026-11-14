param(
    [switch]$Uninstall,
    [switch]$Purge
)

# Tether installer for Windows — one command, no admin rights.
#
#   irm https://raw.githubusercontent.com/AMG555/Tether/main/scripts/install.ps1 | iex
#
# Installs to %LOCALAPPDATA%\Tether\bin, verifies the SHA-256, and adds that folder to your
# user PATH (the current user only — no administrator prompt, no machine-wide change).

$ErrorActionPreference = 'Stop'
$Repo     = if ($env:TETHER_REPO)    { $env:TETHER_REPO }    else { 'AMG555/Tether' }
$BinDir   = if ($env:TETHER_BIN_DIR) { $env:TETHER_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Tether\bin' }
$Version  = if ($env:TETHER_VERSION) { $env:TETHER_VERSION } else { 'latest' }

if ($Uninstall) {
    Write-Host "running tether uninstallation..." -ForegroundColor Cyan
    Get-Process tether, tether-desktop -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    $startupDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup'
    Remove-Item (Join-Path $startupDir 'tether.url') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $startupDir 'tether.lnk') -Force -ErrorAction SilentlyContinue
    $desktopDir = [Environment]::GetFolderPath('Desktop')
    Remove-Item (Join-Path $desktopDir 'Tether.lnk') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $desktopDir 'Tether Desktop.lnk') -Force -ErrorAction SilentlyContinue
    $programsDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
    Remove-Item (Join-Path $programsDir 'Tether.lnk') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $programsDir 'Tether Desktop.lnk') -Force -ErrorAction SilentlyContinue
    if (Test-Path $BinDir) {
        Remove-Item (Join-Path $BinDir 'tether.exe') -Force -ErrorAction SilentlyContinue
        Remove-Item (Join-Path $BinDir 'tether-desktop.exe') -Force -ErrorAction SilentlyContinue
    }
    if ($Purge) {
        $vault = Join-Path $env:LOCALAPPDATA 'tether_data'
        Remove-Item $vault -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "  purged local vault data"
    }
    Write-Host "uninstallation complete." -ForegroundColor Green
    return
}

Write-Host "tether installer"
Write-Host "  platform  windows / $($env:PROCESSOR_ARCHITECTURE)"
Write-Host "  target    $BinDir"

$asset = 'tether-windows-x86_64.zip'
$base  = if ($env:TETHER_BASE_URL) {
    $env:TETHER_BASE_URL.TrimEnd('/')
} elseif ($Version -eq 'latest') {
    "https://github.com/$Repo/releases/latest/download"
} else {
    "https://github.com/$Repo/releases/download/$Version"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tether-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp -Force | Out-Null

try {
    Write-Host "  fetching  $asset"
    $zip = Join-Path $tmp $asset
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $zip -UseBasicParsing

    # Verify before extracting anything.
    $sumUrl = "$base/checksums.txt"
    $checksums = (Invoke-WebRequest -Uri $sumUrl -UseBasicParsing).Content
    $checksumPattern = "^\s*([0-9a-fA-F]{64})\s+\*?$([regex]::Escape($asset))\s*$"
    $line = $checksums -split "\r?\n" |
        Where-Object { $_ -match $checksumPattern } |
        Select-Object -First 1
    if (-not $line) {
        throw "checksums.txt has no valid SHA-256 entry for $asset. Refusing to install."
    }

    $want = [regex]::Match($line, $checksumPattern).Groups[1].Value.ToLowerInvariant()
    $got  = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant()
    if ($want -ne $got) {
        throw "checksum mismatch: expected $want, got $got. Refusing to install."
    }
    Write-Host "  checksum  ok"

    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null

    $exe = Get-ChildItem -Path $tmp -Recurse -Filter 'tether.exe' | Select-Object -First 1
    if (-not $exe) { throw 'the archive did not contain tether.exe' }

    $dest = Join-Path $BinDir 'tether.exe'
    if (Test-Path $dest) { Write-Host "  upgrade   replacing the existing install" }
    Copy-Item $exe.FullName $dest -Force

    Write-Host ""
    Write-Host "installed: $dest"

    # User PATH only. No admin, no machine-wide change.
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -notlike "*$BinDir*") {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$BinDir", 'User')
        Write-Host "added $BinDir to your user PATH (open a new terminal to use it)"
    }

    # Create Desktop and Start Menu shortcuts with global hotkey [Ctrl+Alt+T]
    try {
        $wsh = New-Object -ComObject WScript.Shell
        $desktopDir = [Environment]::GetFolderPath('Desktop')
        $shortcut = $wsh.CreateShortcut((Join-Path $desktopDir 'Tether.lnk'))
        $shortcut.TargetPath = $dest
        $shortcut.WorkingDirectory = $BinDir
        $shortcut.Description = "Tether - 100% offline crash recovery and companion"
        $shortcut.Hotkey = "CTRL+ALT+T"
        $shortcut.Save()

        $programsDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
        $startShortcut = $wsh.CreateShortcut((Join-Path $programsDir 'Tether.lnk'))
        $startShortcut.TargetPath = $dest
        $startShortcut.WorkingDirectory = $BinDir
        $startShortcut.Description = "Tether - 100% offline crash recovery and companion"
        $startShortcut.Save()
        Write-Host "created desktop shortcut with global hotkey [Ctrl+Alt+T]"
    } catch {
        # Non-fatal if COM object fails in non-interactive environment
    }

    Write-Host ""
    Write-Host "next:"
    Write-Host "  tether setup"
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
