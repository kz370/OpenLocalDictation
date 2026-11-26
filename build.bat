@echo off
setlocal EnableExtensions
rem ---------------------------------------------------------------------------
rem  Open Local Dictation - build the release exe, a portable folder and (when
rem  Inno Setup is installed) a setup file, a portable zip and the release notes.
rem  Output: release\v<version>\
rem
rem  Needs: Rust (MSVC toolchain) and Node.js. Builds use half the CPU threads.
rem ---------------------------------------------------------------------------
set "ROOT=%~dp0"
for /f "tokens=2 delims== " %%v in ('findstr /b /c:"version" "%ROOT%src-tauri\Cargo.toml"') do if not defined VERSION set "VERSION=%%~v"
set "DIST=%ROOT%release\v%VERSION%"
set "TARGET=%ROOT%src-tauri\target\release"

set /a JOBS=%NUMBER_OF_PROCESSORS% / 2
if %JOBS% LSS 1 set JOBS=1

rem sherpa-onnx (the speech engine) is a prebuilt archive its build script downloads.
rem Keep a copy in .build-cache\ so a cargo clean does not download it again.
if exist "%ROOT%.build-cache\sherpa-onnx" set "SHERPA_ONNX_ARCHIVE_DIR=%ROOT%.build-cache\sherpa-onnx"

echo [1/3] Building frontend...
pushd "%ROOT%"
if not exist "%ROOT%node_modules" call npm.cmd install || goto :fail
call npm.cmd run build || goto :fail
popd

echo [2/3] Building release exe (%JOBS% jobs)...
pushd "%ROOT%src-tauri"
cargo build --release --jobs %JOBS% || goto :fail
popd

echo [3/3] Packaging...
if not exist "%DIST%" mkdir "%DIST%"
set "PORTABLE=%DIST%\Open-Local-Dictation-%VERSION%-portable"
if exist "%PORTABLE%" rmdir /s /q "%PORTABLE%"
mkdir "%PORTABLE%"
copy /y "%TARGET%\local-dictation.exe" "%PORTABLE%\Open Local Dictation.exe" >nul
for %%d in (sherpa-onnx-c-api sherpa-onnx-cxx-api onnxruntime onnxruntime_providers_shared) do copy /y "%TARGET%\%%d.dll" "%PORTABLE%\" >nul

set "ISCC="
for %%p in ("%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" "%ProgramFiles%\Inno Setup 6\ISCC.exe" "%ProgramFiles%\Inno Setup 7\ISCC.exe" "%LocalAppData%\Programs\Inno Setup 6\ISCC.exe") do if not defined ISCC if exist "%%~p" set "ISCC=%%~p"
if not defined ISCC for /f "delims=" %%p in ('where iscc 2^>nul') do if not defined ISCC set "ISCC=%%p"
if defined ISCC (
  "%ISCC%" /Q "/DAppVersion=%VERSION%" "/DSourceExe=%TARGET%\local-dictation.exe" "/DLibDir=%TARGET%" "/DOutputDir=%DIST%" "%ROOT%local-dictation.iss" || goto :fail
) else (
  echo       Inno Setup not found - skipping the setup file, the portable folder is ready.
)
rem Portable zip and the release notes (written by bump-version.ps1) next to the setup.
if exist "%DIST%\Open-Local-Dictation-%VERSION%-portable.zip" del /q "%DIST%\Open-Local-Dictation-%VERSION%-portable.zip"
powershell -NoProfile -Command "Compress-Archive -Path '%PORTABLE%\*' -DestinationPath '%DIST%\Open-Local-Dictation-%VERSION%-portable.zip' -Force" || goto :fail
if exist "%ROOT%release-notes\v%VERSION%.md" copy /y "%ROOT%release-notes\v%VERSION%.md" "%DIST%\RELEASE_NOTES.md" >nul
echo Done: %DIST%
exit /b 0

:fail
echo [x] Build failed.
exit /b 1
