@echo off
setlocal enabledelayedexpansion
rem ============================================================
rem  dev.bat - Dshnext dev launcher (repo root; see README)
rem
rem  Usage:
rem    dev.bat                       cargo run --release, normal window
rem    dev.bat -d   [app flags]      debug profile (compiles faster, runs slower)
rem    dev.bat -l   [app flags]      RUST_LOG=dshnext=debug (logs every Message)
rem    dev.bat --page env            app flags pass through to dshnext:
rem        --page home|profiles|plugins|env|console|settings
rem        --tall   --theme dark|light
rem        --shot out.png --after 4000
rem        --switch-to settings --switch-at 4000
rem        --autotest --drawlog   --e2e
rem
rem  Always does two things the manual commands forget:
rem    1. bypasses the dead 127.0.0.1:7890 proxy (no_proxy=*)
rem    2. kills a stale dshnext.exe, else cargo build fails with os error 5
rem
rem  Note: cmd's SHIFT does NOT update %* (documented), so the app args
rem  are rebuilt one by one in :collect instead of passed via %*.
rem ============================================================
cd /d "%~dp0"
set no_proxy=*
set NO_PROXY=*

set "PROFILE=--release"
:parse
if /i "%~1"=="-d" (set "PROFILE=" & shift & goto :parse)
if /i "%~1"=="-l" (set "RUST_LOG=dshnext=debug" & shift & goto :parse)

set "APPARGS="
:collect
if "%~1"=="" goto :launch
set "APPARGS=!APPARGS! %1"
shift
goto :collect

:launch
taskkill /F /IM dshnext.exe >nul 2>&1
echo [dev] cargo run %PROFILE% --%APPARGS%
cargo run %PROFILE% --%APPARGS%
exit /b %ERRORLEVEL%
