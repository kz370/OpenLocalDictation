@echo off
setlocal
rem ---------------------------------------------------------------------------
rem  Bumps the app version everywhere and writes the release notes.
rem  bump-version.ps1 does the work; this is the cmd.exe / double-click friendly
rem  entry point and needs no PowerShell execution-policy change.
rem
rem  Usage:   scripts\bump-version.bat patch ^| minor ^| major ^| X.Y.Z  [-DryRun] [-NoNotes]
rem  Example: scripts\bump-version.bat patch -DryRun
rem ---------------------------------------------------------------------------
if "%~1"=="" (
  echo Usage: bump-version.bat patch ^| minor ^| major ^| X.Y.Z  [-DryRun] [-NoNotes]
  exit /b 2
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0bump-version.ps1" %*
exit /b %ERRORLEVEL%
