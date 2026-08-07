@echo off
setlocal
echo Building release executable...
cargo build --release
if errorlevel 1 exit /b %errorlevel%

copy /Y target\release\lg-translate.exe translate.exe >nul
if errorlevel 1 exit /b %errorlevel%

set PAIRS=%*
if "%PAIRS%"=="" set PAIRS=all

echo.
echo Downloading translation models...
rem ko-en and ru-en need a one-time Python conversion step, so a missing Python
rem fails those pairs. Keep going: the summary says what worked and what did not.
for %%P in (%PAIRS%) do translate.exe --download-model %%P

echo.
echo Run: translate.exe
