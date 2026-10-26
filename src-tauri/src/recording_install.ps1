param([Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'
New-Item -ItemType Directory -Path $Destination -Force | Out-Null
$archive = Join-Path $Destination 'ffmpeg.download.zip'
$temporary = Join-Path $Destination 'ffmpeg.pending.exe'
try {
    Invoke-WebRequest -UseBasicParsing -Uri 'https://github.com/GyanD/codexffmpeg/releases/download/9.0.2/ffmpeg-9.0.2-essentials_build.zip' -OutFile $archive -TimeoutSec 600
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne '60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba') { throw 'FFmpeg checksum verification failed. Nothing was installed.' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $binary = @($zip.Entries | Where-Object { $_.FullName -eq 'ffmpeg-9.0.2-essentials_build/bin/ffmpeg.exe' })
        if ($binary.Count -ne 1) { throw 'Unexpected FFmpeg archive layout' }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($binary[0], $temporary, $true)
        # Retain upstream notices and source information alongside the executable.
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName -match '^ffmpeg-9\.0\.2-essentials_build/(LICENSE|README)(\.[A-Za-z]+)?$') {
                [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $Destination $entry.Name), $true)
            }
        }
    } finally { $zip.Dispose() }
    Move-Item -LiteralPath $temporary -Destination (Join-Path $Destination 'ffmpeg.exe') -Force
    'FFmpeg 9.0.2 essentials (Gyan), GPLv3. https://github.com/GyanD/codexffmpeg/releases/tag/9.0.2 | Sources: https://www.gyan.dev/ffmpeg/builds/ | https://ffmpeg.org/' | Set-Content -LiteralPath (Join-Path $Destination 'SOURCE.txt')
} finally {
    if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive }
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
}
