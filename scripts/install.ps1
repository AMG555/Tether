# Savepoint installer for Windows — one command, no admin rights.
#
#   irm https://raw.githubusercontent.com/USER/savepoint/main/scripts/install.ps1 | iex
#
# Installs to %LOCALAPPDATA%\Savepoint\bin, verifies the SHA-256, and adds that folder to your
# user PATH (the current user only — no administrator prompt, no machine-wide change).

$ErrorActionPreference = 'Stop'
$Repo     = if ($env:SAVEPOINT_REPO)    { $env:SAVEPOINT_REPO }    else { 'USER/savepoint' }
$BinDir   = if ($env:SAVEPOINT_BIN_DIR) { $env:SAVEPOINT_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Savepoint\bin' }
$Version  = if ($env:SAVEPOINT_VERSION) { $env:SAVEPOINT_VERSION } else { 'latest' }

Write-Host "savepoint installer"
Write-Host "  platform  windows / $($env:PROCESSOR_ARCHITECTURE)"
Write-Host "  target    $BinDir"

$asset = 'savepoint-windows-x86_64.zip'
$base  = if ($Version -eq 'latest') {
    "https://github.com/$Repo/releases/latest/download"
} else {
    "https://github.com/$Repo/releases/download/$Version"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("savepoint-" + [guid]::NewGuid())
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

    $exe = Get-ChildItem -Path $tmp -Recurse -Filter 'savepoint.exe' | Select-Object -First 1
    if (-not $exe) { throw 'the archive did not contain savepoint.exe' }

    $dest = Join-Path $BinDir 'savepoint.exe'
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
    Write-Host "  savepoint setup"
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
