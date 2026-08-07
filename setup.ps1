$ErrorActionPreference = "Stop"

Write-Host "Building release executable..."
cargo build --release

Copy-Item ".\target\release\offline-translator.exe" ".\translate.exe" -Force

Write-Host ""
Write-Host "Downloading translation model..."
.\translate.exe --download-model

Write-Host ""
Write-Host "Done."
Write-Host "Run: .\translate.exe"
