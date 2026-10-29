param([ValidateSet('core','full','both')][string]$Variant = 'core', [switch]$SkipNativeFetch)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
# Do not terminate an active conversion to release the executable lock.
$running = Get-Process zoriapdf-desktop -ErrorAction SilentlyContinue
if ($running) {
    throw 'ZoriaPDF is running. Close the application normally before building.'
}

if (!$SkipNativeFetch) {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'fetch-native.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Native dependency verification failed' }
}
& npm.cmd ci
if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed' }
& cargo test -p pdf-core
if ($LASTEXITCODE -ne 0) { throw 'Native PDF regression tests failed' }

$resources = Join-Path $root 'src-tauri\resources'
$bundledOcr = Join-Path $resources 'ocr'
$stage = Join-Path $root 'ocr-stage\pack'

function Build-Installer($withOcr, $label) {
    Remove-Item $bundledOcr -Recurse -Force -ErrorAction SilentlyContinue
    # The core bundle carries PDFium only; make sure no OCR payload survives in resources.
    Get-ChildItem $resources -File | Where-Object { $_.Name -ne 'pdfium.dll' } | Remove-Item -Force
    Remove-Item (Join-Path $resources 'tessdata') -Recurse -Force -ErrorAction SilentlyContinue
    if ($withOcr) {
        if (!(Test-Path $stage)) { throw 'OCR pack missing. Run scripts/fetch-native.ps1 first.' }
        Copy-Item $stage $bundledOcr -Recurse -Force
    }
    & npm.cmd run tauri -- build --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw "Windows installer build failed ($label)" }
    $bundle = Join-Path $root 'target\release\bundle\nsis'
    $version = (Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
    $produced = Get-Item (Join-Path $bundle "ZoriaPDF_${version}_x64-setup.exe")
    $renamed = Join-Path $bundle ("ZoriaPDF-{0}-{1}-x64-setup.exe" -f $version, $label)
    if (Test-Path $renamed) { Remove-Item $renamed -Force }
    Move-Item $produced.FullName $renamed
    $sizeMb = [math]::Round((Get-Item $renamed).Length / 1MB, 1)
    Write-Output ("INSTALLER {0}: {1} MB -> {2}" -f $label, $sizeMb, $renamed)
}

if ($Variant -eq 'core' -or $Variant -eq 'both') { Build-Installer $false 'core' }
if ($Variant -eq 'full' -or $Variant -eq 'both') { Build-Installer $true 'full' }
Remove-Item $bundledOcr -Recurse -Force -ErrorAction SilentlyContinue
Write-Output 'WINDOWS BUILD COMPLETE'
