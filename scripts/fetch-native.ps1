param([switch]$SkipOcr)
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$root = Split-Path $PSScriptRoot -Parent
$resources = Join-Path $root 'src-tauri\resources'
$cache = Join-Path $root '.native-cache'
$ocrStage = Join-Path $root 'ocr-stage'
New-Item -ItemType Directory -Force $resources,$cache | Out-Null

function Fetch-Verified($url, $file, $sha256) {
    if (!(Test-Path $file)) { Invoke-WebRequest -UseBasicParsing $url -OutFile $file }
    $actual = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $sha256) { throw "Native dependency checksum mismatch: $file. Expected $sha256, got $actual" }
}

# --- PDFium (core runtime, always bundled) ----------------------------------
$pdfArchive = Join-Path $cache 'pdfium-8111-win-x64.tgz'
Fetch-Verified 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8111/pdfium-win-x64.tgz' $pdfArchive '3452d11031050ec65e57f180215b57b9e6d465e3fbedafa8a77ad90885c7d23e'
$pdfDir = Join-Path $cache 'pdfium'
New-Item -ItemType Directory -Force $pdfDir | Out-Null
& tar -xzf $pdfArchive -C $pdfDir
if ($LASTEXITCODE -ne 0) { throw 'PDFium extraction failed' }
Copy-Item (Join-Path $pdfDir 'bin\pdfium.dll') $resources -Force
$licenses = Join-Path $resources 'licenses'
New-Item -ItemType Directory -Force $licenses | Out-Null
Get-ChildItem $pdfDir -File | Where-Object { $_.Name -match 'LICENSE|NOTICE' } | Copy-Item -Destination $licenses -Force
# Older builds staged the OCR runtime directly in resources; it now lives in its own
# optional pack, so clear any leftovers to keep the core bundle small.
Get-ChildItem $resources -File | Where-Object { $_.Name -ne 'pdfium.dll' } | Remove-Item -Force
Remove-Item (Join-Path $resources 'tessdata') -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $resources 'ocr') -Recurse -Force -ErrorAction SilentlyContinue

# --- OCR (optional component, staged outside the installer) ------------------
if (!$SkipOcr) {
    # Administrative MSI extraction only; no global 7-Zip install, no PATH changes.
    $msi = Join-Path $cache '7z2605-x64.msi'
    Fetch-Verified 'https://github.com/ip7z/7zip/releases/download/26.05/7z2605-x64.msi' $msi '3fe4ab99fa15629c652944309a2b2bc6e41fc933fc6b63f5df1ebcf3aea68263'
    $sevenDir = Join-Path $cache '7zip'
    $seven = Join-Path $sevenDir 'Files\7-Zip\7z.exe'
    if (!(Test-Path $seven)) {
        $proc = Start-Process msiexec.exe -ArgumentList @('/a', ('"' + $msi + '"'), '/qn', ('TARGETDIR="' + $sevenDir + '"')) -Wait -PassThru
        if ($proc.ExitCode -ne 0) { throw "7-Zip administrative extraction failed: $($proc.ExitCode)" }
    }
    if (!(Test-Path $seven)) { throw '7-Zip extractor not found after extraction' }

    $ocrInstaller = Join-Path $cache 'tesseract-5.5.3.exe'
    Fetch-Verified 'https://github.com/tesseract-ocr/tesseract/releases/download/5.5.3/tesseract-ocr-w64-setup-5.5.3.20260724.exe' $ocrInstaller 'bee9e3434bd94fd65387d9be28cd467a41f61b1275383b55b0f59a1331270ae4'
    $ocrExtract = Join-Path $cache 'tesseract'
    & $seven x $ocrInstaller "-o$ocrExtract" -y | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Tesseract extraction failed' }

    # Exact import closure of tesseract.exe + libtesseract + libleptonica.
    # Everything outside this list (ICU and unused codecs) is dropped:
    # 108.6 MB -> 54.7 MB, verified by an end-to-end tur+eng OCR run.
    $ocrRuntime = @(
        'tesseract.exe', 'libtesseract-5.dll', 'libleptonica-6.dll', 'libstdc++-6.dll',
        'libwinpthread-1.dll', 'libgcc_s_seh-1.dll', 'libintl-8.dll', 'zlib1.dll',
        'libjpeg-8.dll', 'libpng16-16.dll', 'libtiff-6.dll', 'libwebp-7.dll', 'libwebpmux-3.dll',
        'libsharpyuv-0.dll', 'libdeflate.dll', 'libopenjp2-7.dll', 'libgif-7.dll',
        'libjbig-0.dll', 'libarchive-13.dll', 'libb2-1.dll', 'libbz2-1.dll', 'liblz4.dll',
        'liblzma-5.dll', 'libzstd.dll', 'libexpat-1.dll', 'libiconv-2.dll', 'libidn2-0.dll',
        'libunistring-5.dll', 'libcurl-4.dll', 'libpsl-5.dll', 'libssh2-1.dll',
        'libbrotlicommon.dll', 'libbrotlidec.dll', 'libLerc.dll'
    )
    $packDir = Join-Path $ocrStage 'pack'
    Remove-Item $packDir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $packDir | Out-Null
    foreach ($name in $ocrRuntime) {
        $found = Get-ChildItem $ocrExtract -Recurse -File -Filter $name -ErrorAction SilentlyContinue | Select-Object -First 1
        if (!$found) { throw "OCR runtime file missing from the Tesseract package: $name" }
        Copy-Item $found.FullName $packDir -Force
    }
    $tessdata = Join-Path $packDir 'tessdata'
    New-Item -ItemType Directory -Force $tessdata | Out-Null
    $languageHashes = @{
        eng = '7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2'
        tur = '7393381111e1152420fc4092cb44eef4237580d21b92bf30d7d221aad192c6b7'
    }
    foreach ($lang in @('eng','tur')) {
        Fetch-Verified "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/87416418657359cb625c412a48b6e1d6d41c29bd/$lang.traineddata" (Join-Path $tessdata "$lang.traineddata") $languageHashes[$lang]
    }
    Fetch-Verified 'https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/87416418657359cb625c412a48b6e1d6d41c29bd/LICENSE' (Join-Path $packDir 'LICENSE-tessdata') 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30'

    # Verify the staged engine actually starts and sees both languages.
    $env:TESSDATA_PREFIX = $tessdata
    & (Join-Path $packDir 'tesseract.exe') --version | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Staged Tesseract did not start' }
    $languages = & (Join-Path $packDir 'tesseract.exe') --list-langs
    if ($LASTEXITCODE -ne 0) { throw 'Staged OCR models unavailable' }
    foreach ($required in @('eng','tur')) {
        if ($languages -notcontains $required) { throw "Staged OCR is missing the $required model" }
    }

    # Deterministic archive: sorted entries with a fixed timestamp, so the published
    # artifact keeps one stable SHA-256 that the application can pin.
    $packZip = Join-Path $ocrStage 'zoriapdf-ocr-1.0.0.zip'
    Remove-Item $packZip -Force -ErrorAction SilentlyContinue
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $stream = [System.IO.File]::Open($packZip, [System.IO.FileMode]::CreateNew)
    $archive = New-Object System.IO.Compression.ZipArchive($stream, [System.IO.Compression.ZipArchiveMode]::Create)
    $fixedTime = New-Object DateTimeOffset (Get-Date '2024-01-01T00:00:00Z')
    $sourceFiles = Get-ChildItem $packDir -Recurse -File | Sort-Object { $_.FullName.Substring($packDir.Length + 1) }
    foreach ($file in $sourceFiles) {
        $relative = $file.FullName.Substring($packDir.Length + 1) -replace '\\', '/'
        $entry = $archive.CreateEntry($relative, [System.IO.Compression.CompressionLevel]::Optimal)
        $entry.LastWriteTime = $fixedTime
        $entryStream = $entry.Open()
        $input = [System.IO.File]::OpenRead($file.FullName)
        $input.CopyTo($entryStream)
        $input.Dispose()
        $entryStream.Dispose()
    }
    $archive.Dispose()
    $stream.Dispose()
    $hash = (Get-FileHash $packZip -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  zoriapdf-ocr-1.0.0.zip" | Set-Content (Join-Path $ocrStage 'zoriapdf-ocr-1.0.0.zip.sha256') -Encoding ASCII
    Write-Output ("OCR PACK READY: {0} MB ({1} files)" -f [math]::Round((Get-Item $packZip).Length/1MB,1), $sourceFiles.Count)
    Write-Output "OCR PACK SHA256: $hash"
}

Write-Output 'CORE NATIVE DEPENDENCIES READY: PDFium 8111'
