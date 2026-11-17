# Encodes frame PNGs into docs/how-it-works.gif
param(
    [string]$FramesDir = "docs/frames",
    [string]$OutputFile = "docs/how-it-works.gif"
)

Add-Type -AssemblyName PresentationCore, WindowsBase

$Root = Split-Path -Parent $MyInvocation.MyCommand.Definition
$FullFramesDir = Join-Path (Split-Path -Parent $Root) $FramesDir
$FullOutputFile = Join-Path (Split-Path -Parent $Root) $OutputFile

$frameFiles = Get-ChildItem -Path $FullFramesDir -Filter "frame_*.png" | Sort-Object Name
if ($frameFiles.Count -eq 0) {
    throw "No frames found in $FullFramesDir"
}

$encoder = New-Object System.Windows.Media.Imaging.GifBitmapEncoder

# Delays in 100ths of a second (100 = 1 sec)
$delays = @(110, 65, 65, 65, 35, 35, 110, 60, 50, 50, 70, 110, 150)

for ($i = 0; $i -lt $frameFiles.Count; $i++) {
    $file = $frameFiles[$i].FullName
    $delay = if ($i -lt $delays.Count) { $delays[$i] } else { 70 }

    $uri = New-Object System.Uri($file)
    $bitmap = New-Object System.Windows.Media.Imaging.BitmapImage
    $bitmap.BeginInit()
    $bitmap.UriSource = $uri
    $bitmap.CacheOption = [System.Windows.Media.Imaging.BitmapCacheOption]::OnLoad
    $bitmap.EndInit()

    $meta = New-Object System.Windows.Media.Imaging.BitmapMetadata("gif")
    $meta.SetQuery("/grctlext/Delay", [UInt16]$delay)
    if ($i -eq 0) {
        $meta.SetQuery("/appext/Application", [byte[]][char[]]"NETSCAPE2.0")
        $meta.SetQuery("/appext/Data", [byte[]](3, 1, 0, 0))
    }

    $frame = [System.Windows.Media.Imaging.BitmapFrame]::Create($bitmap, $null, $meta, $null)
    $encoder.Frames.Add($frame)
}

if (Test-Path $FullOutputFile) {
    Remove-Item $FullOutputFile -Force
}

$stream = [System.IO.File]::OpenWrite($FullOutputFile)
$encoder.Save($stream)
$stream.Close()

$size = (Get-Item $FullOutputFile).Length
Write-Host "Created animated GIF: $FullOutputFile ($size bytes)" -ForegroundColor Green
