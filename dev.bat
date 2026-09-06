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
rem After taskkill, Windows needs a moment to release the exe file lock.
rem Cargo build right away can hit os error 5 (access denied); in a
rem double-click scenario the console flashes and it LOOKS like "no rebuild".
ping -n 2 127.0.0.1 >nul
rem Build and run are SPLIT on purpose: a double-clicked console closes
rem instantly when cargo run fails, hiding the compile error. Build first,
rem pause on failure so it can be read; then launch (build step is a no-op).
rem Pausing on the app's own exit code would be wrong: taskkill-terminated
rem dshnext exits 1, which is not a build failure.
echo [dev] building %PROFILE% ...
cargo build %PROFILE%
if errorlevel 1 (
    echo [dev] build FAILED - see errors above
    pause
    exit /b 1
)
echo [dev] launching dshnext %APPARGS%
cargo run %PROFILE% --%APPARGS%
exit /b %ERRORLEVEL%
