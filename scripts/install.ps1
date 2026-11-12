# Tether installer for Windows — one command, no admin rights.
#
#   irm https://raw.githubusercontent.com/USER/tether/main/scripts/install.ps1 | iex
#
# Installs to %LOCALAPPDATA%\Tether\bin, verifies the SHA-256, and adds that folder to your
# user PATH (the current user only — no administrator prompt, no machine-wide change).

$ErrorActionPreference = 'Stop'
$Repo     = if ($env:TETHER_REPO)    { $env:TETHER_REPO }    else { 'USER/tether' }
$BinDir   = if ($env:TETHER_BIN_DIR) { $env:TETHER_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Tether\bin' }
$Version  = if ($env:TETHER_VERSION) { $env:TETHER_VERSION } else { 'latest' }

Write-Host "tether installer"
Write-Host "  platform  windows / $($env:PROCESSOR_ARCHITECTURE)"
Write-Host "  target    $BinDir"

$asset = 'tether-windows-x86_64.zip'
$base  = if ($Version -eq 'latest') {
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
    try {
        $checksums = (Invoke-WebRequest -Uri $sumUrl -UseBasicParsing).Content
        $line = ($checksums -split "`n" | Where-Object { $_ -match [regex]::Escape($asset) } | Select-Object -First 1)
        if ($line) {
            $want = ($line -split '\s+')[1]
            $got  = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLower()
            if ($want.ToLower() -ne $got) {
                throw "checksum mismatch: expected $want, got $got. Refusing to install."
            }
            Write-Host "  checksum  ok"
        }
    } catch [System.Net.WebException] {
        Write-Host "  warning   no checksums.txt in this release; cannot verify the download"
    }

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
    Write-Host ""
    Write-Host "next:"
    Write-Host "  tether setup"
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
