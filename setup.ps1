param(
    # Language pairs to set up. Defaults to all of them.
    [string[]]$Pairs = @("all")
)

$ErrorActionPreference = "Stop"

Write-Host "Building release executable..."
cargo build --release

Copy-Item ".\target\release\lg-translate.exe" ".\translate.exe" -Force

Write-Host ""
Write-Host "Downloading translation models..."

# ko-en and ru-en need a one-time Python conversion step, so a missing Python
# fails those pairs. Keep going: the summary says what worked and what did not.
$ErrorActionPreference = "Continue"
foreach ($pair in $Pairs) {
    .\translate.exe --download-model $pair
}

Write-Host ""
Write-Host "Run: .\translate.exe"
