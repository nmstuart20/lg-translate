@echo off
setlocal
echo Building release executable...
cargo build --release
if errorlevel 1 exit /b %errorlevel%

copy /Y target\release\offline-translator.exe translate.exe >nul
if errorlevel 1 exit /b %errorlevel%

echo.
echo Downloading translation model...
translate.exe --download-model
if errorlevel 1 exit /b %errorlevel%

echo.
echo Done.
echo Run: translate.exe
