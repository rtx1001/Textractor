param([Parameter(Mandatory=$true)][string]$TranslationRoot, [switch]$CatalogueOnly)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($CatalogueOnly) {
    Invoke-WebRequest -UseBasicParsing -Uri 'https://raw.githubusercontent.com/argosopentech/argospm-index/main/index.json' -OutFile (Join-Path $TranslationRoot 'index.json.download') -TimeoutSec 60
    exit 0
}
$runtime = Join-Path $TranslationRoot 'runtime'
$stage = Join-Path $TranslationRoot 'runtime-install'
$archive = Join-Path $TranslationRoot 'python-runtime.zip'
if (-not (Test-Path (Join-Path $runtime 'python.exe'))) {
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    Invoke-WebRequest -UseBasicParsing -Uri 'https://www.python.org/ftp/python/3.12.10/python-3.12.10-embed-amd64.zip' -OutFile $archive -TimeoutSec 180
    Expand-Archive -LiteralPath $archive -DestinationPath $stage -Force
    "python312.zip`n.`nLib/site-packages`nimport site" | Set-Content -LiteralPath (Join-Path $stage 'python312._pth') -Encoding ASCII
    if (Test-Path $runtime) { throw 'Incomplete translation runtime. Restart Textractor and try again.' }
    Move-Item -LiteralPath $stage -Destination $runtime
    Remove-Item -LiteralPath $archive
}
Expand-Archive -LiteralPath (Join-Path $TranslationRoot 'vc-runtime.zip') -DestinationPath $runtime -Force
& (Join-Path $runtime 'python.exe') (Join-Path $TranslationRoot 'translation.py') setup $TranslationRoot
if ($LASTEXITCODE -ne 0) { throw 'Could not install translation engine dependencies.' }
