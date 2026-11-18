# Winscroller Alt+Tab — Automated Installer & Updater
# Usage:
#   irm https://raw.githubusercontent.com/fruitsaladwindowsai-zammy/Alt-Tab/main/scripts/install.ps1 | iex

param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\WinscrollerAltTab",
    [switch]$NoLaunch,
    [switch]$NoElevate
)

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = "fruitsaladwindowsai-zammy/Alt-Tab"
$ExeUrl = "https://github.com/$Repo/releases/latest/download/winscroller-alt-tab.exe"
$SumsUrl = "https://github.com/$Repo/releases/latest/download/SHA256SUMS.txt"
$TargetExe = Join-Path $InstallDir "winscroller-alt-tab.exe"
$TempExe = Join-Path $env:TEMP "winscroller-alt-tab-download.exe"

Write-Host "=== Winscroller Alt+Tab Installer ===" -ForegroundColor Cyan

# 1. Stop any running instance so the binary can be replaced cleanly
if (Test-Path $TargetExe) {
    Write-Host "Stopping running instance (if active)..." -ForegroundColor Yellow
    Start-Process -FilePath $TargetExe -ArgumentList "--quit" -Wait -NoNewWindow -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 350
}

# 2. Download latest release binary
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Write-Host "Downloading latest winscroller-alt-tab.exe..." -ForegroundColor Cyan
Invoke-WebRequest -UseBasicParsing -Uri $ExeUrl -OutFile $TempExe

# 3. Verify SHA256 checksum if SHA256SUMS.txt is published
try {
    $sumsText = (Invoke-WebRequest -UseBasicParsing -Uri $SumsUrl).Content
    $expected = ($sumsText -split "`n" | Where-Object { $_ -match 'winscroller-alt-tab\.exe$' } | Select-Object -First 1) -replace '\s+.*$', ''
    if ($expected) {
        $actual = (Get-FileHash $TempExe -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $expected.Trim().ToLower()) {
            Remove-Item $TempExe -Force -ErrorAction SilentlyContinue
            throw "SHA256 checksum mismatch! Expected $expected, got $actual"
        }
        Write-Host "SHA256 verified: $actual" -ForegroundColor Green
    }
} catch {
    if ($_.Exception.Message -match "checksum mismatch") { throw }
    Write-Host "Skipping optional checksum check: $($_.Exception.Message)" -ForegroundColor DarkGray
}

Move-Item -Path $TempExe -Destination $TargetExe -Force
Write-Host "Installed to: $TargetExe" -ForegroundColor Green

# 4. Run built-in prerequisite verification & silent auto-installer
Write-Host "Checking and installing Windows runtime prerequisites..." -ForegroundColor Cyan
$prereqProc = Start-Process -FilePath $TargetExe -ArgumentList "--install-prereqs" -Wait -PassThru
if ($prereqProc.ExitCode -ne 0) {
    Write-Host "Warning: Some optional prerequisites reported a warning. See $env:APPDATA\winscroller-alt-tab\prereqs.log" -ForegroundColor Yellow
} else {
    Write-Host "All prerequisites verified!" -ForegroundColor Green
}

# 5. Launch Winscroller Alt+Tab
if (-not $NoLaunch) {
    $launchArgs = @("--replace")
    if ($NoElevate) { $launchArgs += "--no-elevate" }
    Write-Host "Launching Winscroller Alt+Tab..." -ForegroundColor Cyan
    Start-Process -FilePath $TargetExe -ArgumentList $launchArgs
    Write-Host "Winscroller Alt+Tab is running! Right-click its tray icon to enable 'Start with Windows'." -ForegroundColor Green
}
