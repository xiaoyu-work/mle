$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    cargo build --release --locked --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed.' }
    $output = Join-Path (Get-Location) 'dist\MLE'
    New-Item -ItemType Directory -Force -Path "$output\data", "$output\licenses" | Out-Null
    Copy-Item 'target\release\mle.exe' "$output\mle.exe"
    Copy-Item 'data\dictionary.db' "$output\data\dictionary.db"
    Copy-Item 'licenses\ECDICT.txt' "$output\licenses\ECDICT.txt"
    Copy-Item 'README.md' "$output\README.md"
    Copy-Item 'LICENSE' "$output\LICENSE"
    Write-Output "Portable application: $output\mle.exe"
} finally {
    Pop-Location
}
