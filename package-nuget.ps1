# PowerShell script to build, pack, and optionally push Winscroller Alt+Tab to a NuGet registry.
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$PushGithub,
    [switch]$PushNugetOrg,
    [string]$GithubOwner = "fruitsaladwindowsai-zammy",
    [string]$GithubToken,
    [string]$NugetApiKey,
    [string]$OutputDirectory = "target"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $Root

Write-Host "=== Winscroller Alt+Tab NuGet Packager ===" -ForegroundColor Cyan

# 1. Ensure release binary is built
if (-not $SkipBuild) {
    Write-Host "`n[1/3] Building release binary with Cargo..." -ForegroundColor Yellow
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo build failed with exit code $LASTEXITCODE"
    }
}

$ExePath = Join-Path $Root "target\release\winscroller-alt-tab.exe"
if (-not (Test-Path $ExePath)) {
    throw "Release binary not found at $ExePath. Run 'cargo build --release' first."
}

# 2. Ensure nuget.exe is available
Write-Host "`n[2/3] Checking nuget.exe..." -ForegroundColor Yellow
$NugetPath = Join-Path $Root "nuget.exe"
if (-not (Test-Path $NugetPath)) {
    $ExistingNuget = Get-Command "nuget.exe" -ErrorAction SilentlyContinue
    if ($ExistingNuget) {
        $NugetPath = $ExistingNuget.Source
    } else {
        Write-Host "Downloading latest nuget.exe from dist.nuget.org..." -ForegroundColor Gray
        Invoke-WebRequest -Uri "https://dist.nuget.org/win-x86-commandline/latest/nuget.exe" -OutFile $NugetPath -UseBasicParsing
    }
}

# 3. Pack NuGet package
Write-Host "`n[3/3] Creating NuGet package (.nupkg)..." -ForegroundColor Yellow
if (-not (Test-Path $OutputDirectory)) {
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
}

& $NugetPath pack "winscroller-alt-tab.nuspec" -OutputDirectory $OutputDirectory
if ($LASTEXITCODE -ne 0) {
    throw "nuget pack failed with exit code $LASTEXITCODE"
}

$Nupkg = Get-ChildItem (Join-Path $OutputDirectory "*.nupkg") | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Write-Host "`n[SUCCESS] Package created: $($Nupkg.FullName) ($($Nupkg.Length) bytes)" -ForegroundColor Green

# 4. Push to GitHub Packages NuGet Registry (optional)
if ($PushGithub) {
    if (-not $GithubOwner) {
        throw "Please supply -GithubOwner <your-username-or-org> to push to GitHub Packages"
    }
    if (-not $GithubToken) {
        throw "Please supply -GithubToken <github_pat_with_write_packages> to push to GitHub Packages"
    }
    $SourceUrl = "https://nuget.pkg.github.com/$GithubOwner/index.json"
    Write-Host "`nRegistering GitHub Packages NuGet source: $SourceUrl..." -ForegroundColor Yellow
    & $NugetPath sources Add -Name "GitHub-$GithubOwner" -Source $SourceUrl -UserName $GithubOwner -Password $GithubToken -ErrorAction SilentlyContinue
    Write-Host "Pushing $($Nupkg.Name) to GitHub Packages..." -ForegroundColor Yellow
    & $NugetPath push $Nupkg.FullName -Source "GitHub-$GithubOwner" -SkipDuplicate
}

# 5. Push to NuGet.org (optional)
if ($PushNugetOrg) {
    if (-not $NugetApiKey) {
        throw "Please supply -NugetApiKey <your-api-key> to push to NuGet.org"
    }
    Write-Host "`nPushing $($Nupkg.Name) to NuGet.org..." -ForegroundColor Yellow
    & $NugetPath push $Nupkg.FullName -Source "https://api.nuget.org/v3/index.json" -ApiKey $NugetApiKey -SkipDuplicate
}

Write-Host "`nDone!" -ForegroundColor Cyan
